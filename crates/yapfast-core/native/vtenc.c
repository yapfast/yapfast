/* Hardware H264 encoding for Go Live and the camera on macOS, through
 * VideoToolbox. The same interface as hwenc.c, so Rust drives both alike: I420
 * frames in, Annex B access units out, each NAL unit after a start code.
 *
 * Settings as in h264.c: real time, Main profile, no B-frames so a frame
 * comes out for each frame in, the bitrate capped, a keyframe at least every
 * three seconds and when asked, and parameter sets before each keyframe. */
#include <CoreFoundation/CoreFoundation.h>
#include <CoreMedia/CoreMedia.h>
#include <CoreVideo/CoreVideo.h>
#include <VideoToolbox/VideoToolbox.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "libyuv/convert_from.h"

/* RTP's video clock, in which Rust gives each frame's time. */
enum { CLOCK = 90000 };

/* A keyframe at least this often, in seconds: a new viewer waits that long. */
enum { KEYFRAME_SECONDS = 3 };

/* The most the bitrate may be exceeded by in a second, in percent. */
enum { RATE_LIMIT_PERCENT = 125 };

/* H264's start code, in front of each NAL unit. */
static const unsigned char START_CODE[4] = {0, 0, 0, 1};

struct yapfast_vtenc {
  VTCompressionSessionRef session;
  CFDictionaryRef force_keyframe;
  /* The access unit the callback made of the last frame, and whether it's a keyframe. */
  unsigned char *unit;
  size_t unit_size;
  size_t unit_capacity;
  int unit_key;
  /* The callback failed to make one. */
  int failed;
  unsigned width;
  unsigned height;
  unsigned fps;
};

/* Writes why opening failed, or the encoder's name, into `report`. */
static void say(char *report, size_t size, const char *text, int status) {
  if (!size) return;
  if (status) {
    snprintf(report, size, "%s (%d)", text, status);
  } else {
    snprintf(report, size, "%s", text);
  }
}

static int set_number(VTSessionRef session, CFStringRef key, int64_t value) {
  CFNumberRef number = CFNumberCreate(NULL, kCFNumberSInt64Type, &value);
  OSStatus status = VTSessionSetProperty(session, key, number);
  CFRelease(number);
  return status;
}

/* Aims for `kbps`, capped a little above it over a second. */
static int set_rate(struct yapfast_vtenc *enc, unsigned kbps) {
  int status = set_number(enc->session, kVTCompressionPropertyKey_AverageBitRate,
                          (int64_t)kbps * 1000);
  if (status != noErr) return status;
  int64_t bytes = (int64_t)kbps * 1000 / 8 * RATE_LIMIT_PERCENT / 100;
  int64_t seconds = 1;
  CFNumberRef limit_bytes = CFNumberCreate(NULL, kCFNumberSInt64Type, &bytes);
  CFNumberRef limit_seconds = CFNumberCreate(NULL, kCFNumberSInt64Type, &seconds);
  const void *values[] = {limit_bytes, limit_seconds};
  CFArrayRef limits = CFArrayCreate(NULL, values, 2, &kCFTypeArrayCallBacks);
  status = VTSessionSetProperty(enc->session, kVTCompressionPropertyKey_DataRateLimits, limits);
  CFRelease(limits);
  CFRelease(limit_bytes);
  CFRelease(limit_seconds);
  return status;
}

static int set_boolean(VTSessionRef session, CFStringRef key, int value) {
  return VTSessionSetProperty(session, key, value ? kCFBooleanTrue : kCFBooleanFalse);
}

/* Room for `more` bytes after those the unit holds. */
static int reserve(struct yapfast_vtenc *enc, size_t more) {
  size_t needed = enc->unit_size + more;
  if (needed <= enc->unit_capacity) return 0;
  size_t capacity = enc->unit_capacity ? enc->unit_capacity : 1 << 16;
  while (capacity < needed) capacity *= 2;
  unsigned char *grown = realloc(enc->unit, capacity);
  if (!grown) return -1;
  enc->unit = grown;
  enc->unit_capacity = capacity;
  return 0;
}

static int append_nal(struct yapfast_vtenc *enc, const unsigned char *nal, size_t size) {
  if (reserve(enc, sizeof START_CODE + size) < 0) return -1;
  memcpy(enc->unit + enc->unit_size, START_CODE, sizeof START_CODE);
  memcpy(enc->unit + enc->unit_size + sizeof START_CODE, nal, size);
  enc->unit_size += sizeof START_CODE + size;
  return 0;
}

/* Whether the frame is a keyframe: VideoToolbox marks the others NotSync. */
static int is_keyframe(CMSampleBufferRef sample) {
  CFArrayRef attachments = CMSampleBufferGetSampleAttachmentsArray(sample, false);
  if (!attachments || CFArrayGetCount(attachments) == 0) return 1;
  CFDictionaryRef first = CFArrayGetValueAtIndex(attachments, 0);
  return !CFDictionaryContainsKey(first, kCMSampleAttachmentKey_NotSync) ||
         !CFBooleanGetValue(CFDictionaryGetValue(first, kCMSampleAttachmentKey_NotSync));
}

/* Writes the SPS and PPS before a keyframe's slices. */
static int append_parameter_sets(struct yapfast_vtenc *enc, CMFormatDescriptionRef format) {
  size_t count = 0;
  if (CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format, 0, NULL, NULL, &count, NULL) !=
      noErr) {
    return -1;
  }
  for (size_t index = 0; index < count; index++) {
    const uint8_t *set = NULL;
    size_t size = 0;
    if (CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format, index, &set, &size, NULL,
                                                           NULL) != noErr ||
        append_nal(enc, set, size) < 0) {
      return -1;
    }
  }
  return 0;
}

/* Turns the length prefixed NAL units of `sample` into Annex B. */
static int append_slices(struct yapfast_vtenc *enc, CMSampleBufferRef sample,
                         CMFormatDescriptionRef format) {
  int length_size = 0;
  if (CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format, 0, NULL, NULL, NULL,
                                                         &length_size) != noErr ||
      length_size < 1 || length_size > 4) {
    return -1;
  }
  CMBlockBufferRef block = CMSampleBufferGetDataBuffer(sample);
  size_t total = block ? CMBlockBufferGetDataLength(block) : 0;
  unsigned char *copy = total ? malloc(total) : NULL;
  if (!copy) return -1;
  int result = -1;
  if (CMBlockBufferCopyDataBytes(block, 0, total, copy) == noErr) {
    result = 0;
    size_t at = 0;
    while (result == 0 && at + (size_t)length_size <= total) {
      size_t length = 0;
      for (int byte = 0; byte < length_size; byte++) length = (length << 8) | copy[at + byte];
      at += (size_t)length_size;
      if (length > total - at) {
        result = -1;
      } else {
        result = append_nal(enc, copy + at, length);
        at += length;
      }
    }
  }
  free(copy);
  return result;
}

/* VideoToolbox's callback for each encoded frame. */
static void encoded(void *refcon, void *frame_refcon, OSStatus status, VTEncodeInfoFlags flags,
                    CMSampleBufferRef sample) {
  (void)frame_refcon;
  struct yapfast_vtenc *enc = refcon;
  if (status != noErr) {
    enc->failed = 1;
    return;
  }
  /* A frame the encoder dropped has no sample. */
  if (!sample || (flags & kVTEncodeInfo_FrameDropped)) return;
  CMFormatDescriptionRef format = CMSampleBufferGetFormatDescription(sample);
  if (!format) {
    enc->failed = 1;
    return;
  }
  enc->unit_size = 0;
  enc->unit_key = is_keyframe(sample);
  if ((enc->unit_key && append_parameter_sets(enc, format) < 0) ||
      append_slices(enc, sample, format) < 0) {
    enc->unit_size = 0;
    enc->failed = 1;
  }
}

static CFDictionaryRef source_attributes(unsigned width, unsigned height) {
  int format = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
  int w = (int)width;
  int h = (int)height;
  CFNumberRef format_number = CFNumberCreate(NULL, kCFNumberIntType, &format);
  CFNumberRef width_number = CFNumberCreate(NULL, kCFNumberIntType, &w);
  CFNumberRef height_number = CFNumberCreate(NULL, kCFNumberIntType, &h);
  const void *keys[] = {kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey,
                        kCVPixelBufferHeightKey};
  const void *values[] = {format_number, width_number, height_number};
  CFDictionaryRef attributes = CFDictionaryCreate(
      NULL, keys, values, 3, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
  CFRelease(format_number);
  CFRelease(width_number);
  CFRelease(height_number);
  return attributes;
}

void yapfast_vtenc_free(struct yapfast_vtenc *enc) {
  if (!enc) return;
  if (enc->session) {
    VTCompressionSessionCompleteFrames(enc->session, kCMTimeInvalid);
    VTCompressionSessionInvalidate(enc->session);
    CFRelease(enc->session);
  }
  if (enc->force_keyframe) CFRelease(enc->force_keyframe);
  free(enc->unit);
  free(enc);
}

/* Opens H264 (`codec_id` 0, as Rust numbers it) on the Mac's encoder, and says
 * its name in `report`, or why it didn't open. */
struct yapfast_vtenc *yapfast_vtenc_new(unsigned codec_id, unsigned width, unsigned height,
                                        unsigned fps, unsigned kbps, char *report,
                                        size_t report_size) {
  if (report_size) report[0] = 0;
  if (codec_id != 0 || fps == 0) return NULL;
  struct yapfast_vtenc *enc = calloc(1, sizeof *enc);
  if (!enc) return NULL;
  enc->width = width;
  enc->height = height;
  enc->fps = fps;
  const void *spec_keys[] = {kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder};
  const void *spec_values[] = {kCFBooleanTrue};
  CFDictionaryRef specification = CFDictionaryCreate(
      NULL, spec_keys, spec_values, 1, &kCFTypeDictionaryKeyCallBacks,
      &kCFTypeDictionaryValueCallBacks);
  CFDictionaryRef attributes = source_attributes(width, height);
  OSStatus status = VTCompressionSessionCreate(NULL, (int32_t)width, (int32_t)height,
                                               kCMVideoCodecType_H264, specification, attributes,
                                               NULL, encoded, enc, &enc->session);
  CFRelease(specification);
  CFRelease(attributes);
  if (status != noErr) {
    say(report, report_size, "h264_videotoolbox did not open", status);
    enc->session = NULL;
    yapfast_vtenc_free(enc);
    return NULL;
  }
  int64_t keyframe_interval = (int64_t)fps * KEYFRAME_SECONDS;
  status = set_boolean(enc->session, kVTCompressionPropertyKey_RealTime, 1);
  if (status == noErr) {
    status = VTSessionSetProperty(enc->session, kVTCompressionPropertyKey_ProfileLevel,
                                  kVTProfileLevel_H264_Main_AutoLevel);
  }
  if (status == noErr) {
    status = set_boolean(enc->session, kVTCompressionPropertyKey_AllowFrameReordering, 0);
  }
  if (status == noErr) {
    status = set_number(enc->session, kVTCompressionPropertyKey_MaxKeyFrameInterval,
                        keyframe_interval);
  }
  if (status == noErr) {
    status = set_number(enc->session, kVTCompressionPropertyKey_ExpectedFrameRate, fps);
  }
  if (status == noErr) status = set_rate(enc, kbps);
  /* Speed over quality only matters to the setting's newer systems. */
  set_boolean(enc->session, kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality, 1);
  if (status == noErr) status = VTCompressionSessionPrepareToEncodeFrames(enc->session);
  if (status != noErr) {
    say(report, report_size, "h264_videotoolbox refused its settings", status);
    yapfast_vtenc_free(enc);
    return NULL;
  }
  const void *key = kVTEncodeFrameOptionKey_ForceKeyFrame;
  const void *value = kCFBooleanTrue;
  enc->force_keyframe = CFDictionaryCreate(NULL, &key, &value, 1, &kCFTypeDictionaryKeyCallBacks,
                                           &kCFTypeDictionaryValueCallBacks);
  say(report, report_size, "h264_videotoolbox", 0);
  return enc;
}

/* Marks the picture's colours as BT.601 with limited range, which libyuv
 * converts with, so the stream says so. */
static void tag_colours(CVPixelBufferRef buffer) {
  CVBufferSetAttachment(buffer, kCVImageBufferYCbCrMatrixKey, kCVImageBufferYCbCrMatrix_ITU_R_601_4,
                        kCVAttachmentMode_ShouldPropagate);
  CVBufferSetAttachment(buffer, kCVImageBufferColorPrimariesKey, kCVImageBufferColorPrimaries_SMPTE_C,
                        kCVAttachmentMode_ShouldPropagate);
  CVBufferSetAttachment(buffer, kCVImageBufferTransferFunctionKey,
                        kCVImageBufferTransferFunction_ITU_R_709_2,
                        kCVAttachmentMode_ShouldPropagate);
}

/* Takes a pixel buffer from the session's pool and fills it with the I420 frame as NV12. */
static CVPixelBufferRef make_picture(struct yapfast_vtenc *enc, const unsigned char *y,
                                     int y_stride, const unsigned char *u, int u_stride,
                                     const unsigned char *v, int v_stride) {
  CVPixelBufferPoolRef pool = VTCompressionSessionGetPixelBufferPool(enc->session);
  CVPixelBufferRef buffer = NULL;
  if (!pool || CVPixelBufferPoolCreatePixelBuffer(NULL, pool, &buffer) != kCVReturnSuccess) {
    return NULL;
  }
  if (CVPixelBufferLockBaseAddress(buffer, 0) != kCVReturnSuccess) {
    CVPixelBufferRelease(buffer);
    return NULL;
  }
  int converted = I420ToNV12(
      y, y_stride, u, u_stride, v, v_stride, CVPixelBufferGetBaseAddressOfPlane(buffer, 0),
      (int)CVPixelBufferGetBytesPerRowOfPlane(buffer, 0),
      CVPixelBufferGetBaseAddressOfPlane(buffer, 1),
      (int)CVPixelBufferGetBytesPerRowOfPlane(buffer, 1), (int)enc->width, (int)enc->height);
  CVPixelBufferUnlockBaseAddress(buffer, 0);
  if (converted != 0) {
    CVPixelBufferRelease(buffer);
    return NULL;
  }
  tag_colours(buffer);
  return buffer;
}

/* Encodes one I420 frame into `out`, with `pts` in 90 kHz units. Returns the
 * encoded size, 0 when the encoder gave no frame, -2 when it doesn't fit, or
 * another negative number on failure. `duration` is unused: the frame rate
 * gives it. */
long yapfast_vtenc_encode(struct yapfast_vtenc *enc, const unsigned char *y, int y_stride,
                          const unsigned char *u, int u_stride, const unsigned char *v,
                          int v_stride, long long pts, unsigned long duration, int keyframe,
                          unsigned char *out, size_t capacity, int *is_key) {
  (void)duration;
  CVPixelBufferRef picture =
      make_picture(enc, y, y_stride, u, u_stride, v, v_stride);
  if (!picture) return -1;
  CMTime time = CMTimeMake(pts, CLOCK);
  CMTime frame_duration = CMTimeMake(CLOCK / enc->fps, CLOCK);
  enc->unit_size = 0;
  enc->failed = 0;
  VTEncodeInfoFlags flags = 0;
  OSStatus status = VTCompressionSessionEncodeFrame(
      enc->session, picture, time, frame_duration, keyframe ? enc->force_keyframe : NULL, NULL,
      &flags);
  CVPixelBufferRelease(picture);
  if (status != noErr) return -1;
  /* The callback runs for this frame before this returns. */
  status = VTCompressionSessionCompleteFrames(enc->session, time);
  if (status != noErr || enc->failed) return -1;
  if (enc->unit_size == 0) return 0;
  if (enc->unit_size > capacity) return -2;
  memcpy(out, enc->unit, enc->unit_size);
  *is_key = enc->unit_key;
  return (long)enc->unit_size;
}

/* Changes the bitrate the encoder aims for, as the network allows. Returns
 * 0, or nonzero on failure. */
int yapfast_vtenc_set_bitrate(struct yapfast_vtenc *enc, unsigned kbps) {
  return set_rate(enc, kbps) != noErr;
}
