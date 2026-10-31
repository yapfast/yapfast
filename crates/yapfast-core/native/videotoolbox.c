/* H264 decoding on macOS through VideoToolbox, which is what the Mac's
 * libwebrtc archive leaves out: its FFmpeg decodes audio and nothing else.
 * VideoToolbox is a hardware decoder, so it gives back NV12 — luma, then U and
 * V interleaved — where the rest of Yapfast wants three separate planes. One
 * libyuv pass takes the chroma apart into a buffer the decoder keeps and
 * reuses, so a frame costs no allocation and no second copy.
 *
 * A session belongs to one pair of parameter sets. Live streams repeat them
 * before each keyframe and a stream can change size mid-flight, so the session
 * is built on the first pair seen and only replaced when it won't take the new
 * one. */
#include <stdlib.h>
#include <string.h>

#include <CoreMedia/CMFormatDescription.h>
#include <CoreMedia/CMSampleBuffer.h>
#include <CoreVideo/CVPixelBuffer.h>
#include <VideoToolbox/VTDecompressionProperties.h>
#include <VideoToolbox/VTDecompressionSession.h>
#include <VideoToolbox/VTErrors.h>

#include "libyuv/convert.h"
#include "videotoolbox.h"

/* The widest or tallest picture a stream may send, as in decode.c: a stream
 * can't make Yapfast take more memory than that by claiming a bigger one. */
enum { MAX_SIDE = 8192 };

/* How many decoded pictures wait for the caller. One is taken per access unit,
 * so this only fills when the decoder lets several out at once; it bounds what
 * a file that never asks for a picture can make Yapfast hold on to. */
enum { MOST_PENDING = 16 };

/* How many pictures to hold before handing one out. A file's pictures are
 * decoded out of the order they play, and VideoToolbox hands them back as it
 * decoded them rather than as they play, so the one waiting longest can only be
 * given once nothing still to come can jump ahead of it. H264 encoders hold a
 * few frames for this and no more; four covers the deepest setting in common
 * use, and a file is buffered well past that by the time it plays. */
enum { REORDER = 4 };

/* The microseconds a `pts_us` runs per, which is how a picture's time goes
 * into a sample and comes back out of the callback unchanged. */
static const int32_t PTS_TIMESCALE = 1000000;

/* The NAL units of an access unit that a sample doesn't want. The parameter
 * sets go to the session instead; the rest are access unit delimiters, end
 * markers, padding and the like, which a hardware session trips over. */
static int in_the_way(int type) { return type >= 7 && type <= 13; }

/* NAL units split by data partitioning are a different picture format
 * altogether, which nothing in Yapfast decodes. */
static int partitioned(int type) { return type >= 2 && type <= 4; }

/* Where the parameter sets are, so a session can be made from them. */
struct parameter_sets {
  const uint8_t *sps;
  size_t sps_size;
  const uint8_t *pps;
  size_t pps_size;
};

struct yapfast_vt {
  VTDecompressionSessionRef session;
  /* What the next sample is built with, and what the session was made for. */
  CMFormatDescriptionRef format;
  /* The access unit with each NAL unit's length in front of it, which the
   * sample borrows rather than copies. It stays the same buffer until the
   * decoder is done with the sample, which `settled` sees to first. */
  uint8_t *avcc;
  size_t avcc_size;
  /* What the callback gave back, oldest first, until the caller takes them. */
  CVPixelBufferRef pending[MOST_PENDING];
  int64_t pending_pts[MOST_PENDING];
  int pending_count;
  /* The I420 planes handed out, big enough for the frame now showing. */
  uint8_t *i420;
  size_t i420_size;
  int low_delay;
  /* Whether nothing more will be decoded, so what's waiting can all go out. */
  int flushed;
  /* How many access units have gone in, which is their decode order. */
  uint64_t decoded;
};

/* Walks one access unit's NAL units, Annex-B or prefixed with their lengths. */
struct nal_reader {
  /* Where the next NAL unit's own bytes start, past its start code. */
  const uint8_t *at;
  const uint8_t *end;
  int avcc;
  /* How many bytes hold the length in front of each unit, when `avcc`. */
  int length_size;
};

/* How long the start code at `at` is, or 0. The four-byte one is asked for
 * first, so its leading zero isn't left behind as part of the picture. */
static int start_code(const uint8_t *at, const uint8_t *end) {
  if (end - at >= 4 && at[0] == 0 && at[1] == 0 && at[2] == 0 && at[3] == 1) return 4;
  if (end - at >= 3 && at[0] == 0 && at[1] == 0 && at[2] == 1) return 3;
  return 0;
}

static int nal_next(struct nal_reader *reader, const uint8_t **data, size_t *size) {
  if (reader->at >= reader->end) return 0;
  if (reader->avcc) {
    if (reader->end - reader->at < reader->length_size) return 0;
    size_t length = 0;
    for (int byte = 0; byte < reader->length_size; byte++) {
      length = (length << 8) | reader->at[byte];
    }
    reader->at += reader->length_size;
    if (length == 0 || (size_t)(reader->end - reader->at) < length) return 0;
    *data = reader->at;
    *size = length;
    reader->at += length;
    return 1;
  }
  /* Annex-B: the next start code ends this unit. Emulation prevention bytes
   * keep a start code's pattern out of the picture, so the next one found is
   * really the next unit. */
  const uint8_t *start = reader->at;
  const uint8_t *unit_end = reader->end;
  for (const uint8_t *at = start; at + 3 <= reader->end; at++) {
    int code = start_code(at, reader->end);
    if (!code) continue;
    unit_end = at;
    reader->at = at + code;
    goto found;
  }
  reader->at = reader->end;
found:
  /* Zeros padding the last unit before the end are not part of it. */
  while (unit_end > start && unit_end[-1] == 0) unit_end--;
  *data = start;
  *size = (size_t)(unit_end - start);
  return *size > 0;
}

/* Starts reading `data` as NAL units, Annex-B or prefixed with their lengths,
 * at its first one. */
static void nal_reader_open(struct nal_reader *reader, const uint8_t *data, size_t size,
                            int avcc) {
  reader->end = data + size;
  reader->avcc = avcc;
  reader->length_size = 4;
  reader->at = data;
  if (!avcc) reader->at += start_code(data, data + size);
}

static void remember_sets(struct parameter_sets *sets, int type, const uint8_t *data,
                          size_t size) {
  if (type == 7 && !sets->sps) {
    sets->sps = data;
    sets->sps_size = size;
  } else if (type == 8 && !sets->pps) {
    sets->pps = data;
    sets->pps_size = size;
  }
}

/* Finds both parameter sets in one access unit, and whether it partitions its
 * slices. */
static int read_sets(struct nal_reader *reader, struct parameter_sets *sets) {
  const uint8_t *data;
  size_t size;
  struct nal_reader scan = *reader;
  while (nal_next(&scan, &data, &size)) {
    int type = data[0] & 0x1f;
    if (partitioned(type)) return -1;
    remember_sets(sets, type, data, size);
  }
  return sets->sps && sets->pps;
}

/* Writes the session's description for `sets`, or NULL. High 10-bit, 4:2:2 and
 * 4:4:4 profiles land here too, and VideoToolbox says so by failing. */
static CMFormatDescriptionRef describe(const struct parameter_sets *sets) {
  const uint8_t *each[2] = {sets->sps, sets->pps};
  size_t lengths[2] = {sets->sps_size, sets->pps_size};
  CMFormatDescriptionRef format = NULL;
  /* Four bytes prefix each NAL unit of the samples built with this. */
  if (CMVideoFormatDescriptionCreateFromH264ParameterSets(kCFAllocatorDefault, 2, each, lengths, 4,
                                                         &format) != noErr) {
    return NULL;
  }
  return format;
}

/* The callback's picture, kept until the caller takes it. */
static void decompressed(void *ref_con, void *source_ref_con, OSStatus status,
                         VTDecodeInfoFlags info_flags, CVImageBufferRef image,
                         CMTime presentation, CMTime duration) {
  struct yapfast_vt *vt = ref_con;
  (void)source_ref_con;
  (void)info_flags;
  (void)duration;
  /* A frame the decoder dropped is not a failure: the caller gets none this
   * time and hands it another access unit. */
  if (status != noErr || !image) return;
  CVPixelBufferRef buffer = (CVPixelBufferRef)image;
  int64_t pts = CMTIME_IS_VALID(presentation) && presentation.timescale
                    ? CMTimeConvertScale(presentation, PTS_TIMESCALE,
                                         kCMTimeRoundingMethod_RoundHalfAwayFromZero)
                          .value
                    : -1;
  if (vt->pending_count == MOST_PENDING) {
    /* More than the caller takes: the one waiting longest goes, since it's the
     * one it should have seen already. */
    CVBufferRelease(vt->pending[0]);
    vt->pending_count--;
    for (int index = 0; index < vt->pending_count; index++) {
      vt->pending[index] = vt->pending[index + 1];
      vt->pending_pts[index] = vt->pending_pts[index + 1];
    }
  }
  /* Where it belongs by when it plays, behind any that play no later than it so
   * pictures sharing a time keep the order they came in. */
  int at = vt->pending_count;
  while (at > 0 && vt->pending_pts[at - 1] > pts) at--;
  for (int index = vt->pending_count; index > at; index--) {
    vt->pending[index] = vt->pending[index - 1];
    vt->pending_pts[index] = vt->pending_pts[index - 1];
  }
  vt->pending[at] = CVBufferRetain(buffer);
  vt->pending_pts[at] = pts;
  vt->pending_count++;
}

/* Runs the decoder dry of anything it holds for this call, so the sample's
 * memory is its own again and the scratch can be written over. Not `wait`,
 * which is already the name of a system call here. */
static void settled(struct yapfast_vt *vt) {
  if (vt->session) VTDecompressionSessionWaitForAsynchronousFrames(vt->session);
}

/* Builds a session for `format`, first asking for the hardware one and then,
 * where there isn't one, for whatever VideoToolbox would pick. */
static VTDecompressionSessionRef open_session(CMFormatDescriptionRef format,
                                              struct yapfast_vt *vt) {
  VTDecompressionOutputCallbackRecord callback = {decompressed, vt};
  /* NV12 in shared memory, which is what a hardware decoder writes without
   * converting on the way out. Asking for planar here would either fail the
   * session or make it hand the frame to software. */
  OSType planes = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
  CFNumberRef kind = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt32Type, &planes);
  CFMutableDictionaryRef surfaces =
      CFDictionaryCreateMutable(kCFAllocatorDefault, 0, &kCFTypeDictionaryKeyCallBacks,
                                &kCFTypeDictionaryValueCallBacks);
  const void *keys[] = {kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferIOSurfacePropertiesKey};
  const void *values[] = {kind, surfaces};
  CFDictionaryRef wanted = CFDictionaryCreate(kCFAllocatorDefault, keys, values, 2,
                                              &kCFTypeDictionaryKeyCallBacks,
                                              &kCFTypeDictionaryValueCallBacks);
  VTDecompressionSessionRef session = NULL;
  for (int hardware = 1; hardware && !session; hardware--) {
    CFMutableDictionaryRef decoder = NULL;
    if (hardware) {
      decoder = CFDictionaryCreateMutable(kCFAllocatorDefault, 0, &kCFTypeDictionaryKeyCallBacks,
                                          &kCFTypeDictionaryValueCallBacks);
      CFDictionarySetValue(decoder, kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder,
                           kCFBooleanTrue);
    }
    if (VTDecompressionSessionCreate(kCFAllocatorDefault, format, decoder, wanted, &callback,
                                     &session) != noErr) {
      session = NULL;
    }
    if (decoder) CFRelease(decoder);
  }
  CFRelease(wanted);
  CFRelease(surfaces);
  CFRelease(kind);
  return session;
}

/* Makes `sets` what the decoder decodes next, building a session when the one
 * it has won't take them. */
static int use_sets(struct yapfast_vt *vt, const struct parameter_sets *sets) {
  CMFormatDescriptionRef format = describe(sets);
  if (!format) return -1;
  if (vt->format && CFEqual(vt->format, format)) {
    CFRelease(format);
    return 0;
  }
  if (vt->session && VTDecompressionSessionCanAcceptFormatDescription(vt->session, format)) {
    CFRelease(vt->format);
    vt->format = format;
    return 0;
  }
  VTDecompressionSessionRef session = open_session(format, vt);
  if (!session) {
    CFRelease(format);
    return -1;
  }
  /* The description and the session that was built for it come and go
   * together, so there's only an old one to let go of when there's an old
   * session too. */
  if (vt->session) {
    settled(vt);
    VTDecompressionSessionInvalidate(vt->session);
    CFRelease(vt->session);
    CFRelease(vt->format);
  }
  vt->session = session;
  vt->format = format;
  return 0;
}

/* Makes room for `size` bytes of access unit, growing by doubling so a stream
 * that changes size doesn't realloc every frame. */
static int keep_avcc(struct yapfast_vt *vt, size_t size) {
  if (vt->avcc_size >= size) return 0;
  size_t wanted = vt->avcc_size ? vt->avcc_size : 1 << 14;
  while (wanted < size) wanted *= 2;
  uint8_t *grown = realloc(vt->avcc, wanted);
  if (!grown) return -1;
  vt->avcc = grown;
  vt->avcc_size = wanted;
  return 0;
}

/* Writes the access unit's slices, and anything the decoder reads with them,
 * as NAL units each preceded by four bytes of length, which is what a sample
 * holds. Returns how many bytes it wrote, or a negative number. */
static long rewrite(struct yapfast_vt *vt, struct nal_reader *reader) {
  size_t written = 0;
  const uint8_t *data;
  size_t size;
  while (nal_next(reader, &data, &size)) {
    int type = data[0] & 0x1f;
    if (in_the_way(type)) continue;
    if (partitioned(type)) return -1;
    if (keep_avcc(vt, written + 4 + size) < 0) return -1;
    uint8_t *at = vt->avcc + written;
    at[0] = (uint8_t)(size >> 24);
    at[1] = (uint8_t)(size >> 16);
    at[2] = (uint8_t)(size >> 8);
    at[3] = (uint8_t)size;
    memcpy(at + 4, data, size);
    written += 4 + size;
  }
  return (long)written;
}

struct yapfast_vt *yapfast_vt_new(int low_delay) {
  struct yapfast_vt *vt = calloc(1, sizeof *vt);
  if (vt) vt->low_delay = low_delay;
  return vt;
}

void yapfast_vt_free(struct yapfast_vt *vt) {
  if (!vt) return;
  for (int index = 0; index < vt->pending_count; index++) CVBufferRelease(vt->pending[index]);
  if (vt->session) {
    VTDecompressionSessionInvalidate(vt->session);
    CFRelease(vt->session);
  }
  if (vt->format) CFRelease(vt->format);
  free(vt->avcc);
  free(vt->i420);
  free(vt);
}

int yapfast_vt_set_extradata(struct yapfast_vt *vt, const uint8_t *data, size_t size) {
  if (!vt || !data || size < 4) return 0; /* nothing to prime with; frames say */
  struct parameter_sets sets = {0};
  if (data[0] == 1 && size >= 7) {
    /* An H264Config: each parameter set preceded by two bytes of length. */
    unsigned count = data[5] & 0x1f;
    size_t at = 6;
    for (unsigned index = 0; index < count && at + 2 <= size; index++) {
      size_t length = ((size_t)data[at] << 8) | data[at + 1];
      at += 2;
      if (at + length > size) return -1;
      remember_sets(&sets, data[at] & 0x1f, data + at, length);
      at += length;
    }
    if (at >= size) return -1;
    count = data[at++];
    for (unsigned index = 0; index < count && at + 2 <= size; index++) {
      size_t length = ((size_t)data[at] << 8) | data[at + 1];
      at += 2;
      if (at + length > size) return -1;
      remember_sets(&sets, data[at] & 0x1f, data + at, length);
      at += length;
    }
  } else {
    /* Some files keep the parameter sets in the same Annex-B form as frames. */
    struct nal_reader reader;
    nal_reader_open(&reader, data, size, 0);
    if (read_sets(&reader, &sets) < 0) return -1;
  }
  if (!sets.sps || !sets.pps) return 0;
  return use_sets(vt, &sets);
}

/* The picture in `buffer`, as the I420 planes Yapfast works in. One libyuv pass
 * takes NV12's interleaved chroma apart into a buffer the decoder keeps and
 * reuses, so a picture costs no allocation and no second copy. Returns 1 with
 * the planes, and 0 when the picture can't be read. */
static int fill(struct yapfast_vt *vt, CVPixelBufferRef buffer, struct yapfast_vt_frame *frame) {
  /* The clean aperture is where the picture really is: a frame of 1080 lines
   * is coded in 1088, and its origin is the lower left, so the rows it leaves
   * above it are the ones the coded height has but the picture doesn't. */
  int coded_width = (int)CVPixelBufferGetWidth(buffer);
  int coded_height = (int)CVPixelBufferGetHeight(buffer);
  int width = coded_width, height = coded_height, left = 0, top = 0;
  CGRect clean = CVImageBufferGetCleanRect((CVImageBufferRef)buffer);
  if (clean.size.width > 0 && clean.size.height > 0 && clean.origin.x >= 0 &&
      clean.origin.y >= 0 && clean.origin.x + clean.size.width <= coded_width &&
      clean.origin.y + clean.size.height <= coded_height) {
    left = (int)clean.origin.x;
    width = (int)clean.size.width;
    height = (int)clean.size.height;
    top = coded_height - (int)(clean.origin.y + clean.size.height);
    if (top < 0) top = 0;
  }
  if (width <= 0 || height <= 0 || width > MAX_SIDE || height > MAX_SIDE) return 0;
  int chroma_width = (width + 1) / 2;
  size_t luma = (size_t)width * (size_t)height;
  size_t chroma = (size_t)chroma_width * (size_t)((height + 1) / 2);
  if (vt->i420_size < luma + 2 * chroma) {
    uint8_t *grown = realloc(vt->i420, luma + 2 * chroma);
    if (!grown) return 0;
    vt->i420 = grown;
    vt->i420_size = luma + 2 * chroma;
  }
  if (CVPixelBufferLockBaseAddress(buffer, kCVPixelBufferLock_ReadOnly) != noErr) return 0;
  int y_stride = (int)CVPixelBufferGetBytesPerRowOfPlane(buffer, 0);
  int uv_stride = (int)CVPixelBufferGetBytesPerRowOfPlane(buffer, 1);
  const uint8_t *src_y = (const uint8_t *)CVPixelBufferGetBaseAddressOfPlane(buffer, 0) +
                         (size_t)top * (size_t)y_stride + (size_t)left;
  /* NV12's second plane holds U and V in turn, so a chroma pixel costs two
   * bytes and one of its rows stands for two of luma. */
  const uint8_t *src_uv = (const uint8_t *)CVPixelBufferGetBaseAddressOfPlane(buffer, 1) +
                          (size_t)(top / 2) * (size_t)uv_stride + (size_t)(left / 2) * 2;
  uint8_t *dst_y = vt->i420, *dst_u = dst_y + luma, *dst_v = dst_u + chroma;
  int failed = NV12ToI420(src_y, y_stride, src_uv, uv_stride, dst_y, width, dst_u, chroma_width,
                          dst_v, chroma_width, width, height);
  CVPixelBufferUnlockBaseAddress(buffer, kCVPixelBufferLock_ReadOnly);
  if (failed) return 0;
  /* What the picture says its colours are; -1 lets the caller fall back on the
   * height, as it does for the pictures FFmpeg decodes. */
  int bt709 = -1;
  CFTypeRef matrix = CVBufferCopyAttachment(buffer, kCVImageBufferYCbCrMatrixKey, NULL);
  if (matrix) {
    if (CFEqual(matrix, kCVImageBufferYCbCrMatrix_ITU_R_709_2)) bt709 = 1;
    else if (CFEqual(matrix, kCVImageBufferYCbCrMatrix_ITU_R_601_4)) bt709 = 0;
    CFRelease(matrix);
  }
  *frame = (struct yapfast_vt_frame){
      .width = width,
      .height = height,
      .data = {dst_y, dst_u, dst_v},
      .stride = {width, chroma_width, chroma_width},
      .bt709 = bt709,
      .full_range = CVPixelBufferGetPixelFormatType(buffer) ==
                    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
  };
  return 1;
}

/* The oldest picture the callback left, if any. One comes off each call, which
 * is what both callers want: a picture per access unit, or per turn at the
 * file. One that can't be read is dropped rather than held, so no caller waits
 * on it forever. */
static int take(struct yapfast_vt *vt, struct yapfast_vt_frame *frame, int64_t *out_pts_us) {
  if (!vt->pending_count) return 0;
  /* A live picture shows as it comes; a file's wait until nothing can still
   * come in front of them. */
  if (!vt->low_delay && !vt->flushed && vt->pending_count <= REORDER) return 0;
  CVPixelBufferRef buffer = vt->pending[0];
  int64_t pts = vt->pending_pts[0];
  int got = fill(vt, buffer, frame);
  CVBufferRelease(buffer);
  vt->pending_count--;
  for (int index = 0; index < vt->pending_count; index++) {
    vt->pending[index] = vt->pending[index + 1];
    vt->pending_pts[index] = vt->pending_pts[index + 1];
  }
  if (got && out_pts_us) *out_pts_us = pts;
  return got;
}

int yapfast_vt_decode(struct yapfast_vt *vt, const uint8_t *data, size_t size, int avcc,
                      int64_t pts_us) {
  if (!vt || !data || size == 0) return 0;
  struct nal_reader reader;
  nal_reader_open(&reader, data, size, avcc);
  struct parameter_sets sets = {0};
  if (read_sets(&reader, &sets) < 0) return -1;
  if (sets.sps && sets.pps && use_sets(vt, &sets) < 0) return -1;
  /* Nothing to build a session from yet: joining a stream mid-way, the first
   * frames carry no parameter sets. Ask again with the next keyframe. */
  if (!vt->session) return 0;
  long written = rewrite(vt, &reader);
  if (written < 0) return -1;
  if (written == 0) return 0; /* the access unit was all parameter sets */

  CMBlockBufferRef block = NULL;
  if (CMBlockBufferCreateWithMemoryBlock(kCFAllocatorDefault, vt->avcc, (size_t)written,
                                         kCFAllocatorNull, NULL, 0, (size_t)written, 0,
                                         &block) != noErr) {
    return -1;
  }
  /* A live stream says nothing about when its pictures play; the sample still
   * wants a time, and nothing reads this one's. */
  CMTime at = CMTimeMake(pts_us >= 0 ? pts_us : 0, PTS_TIMESCALE);
  /* The order they arrived in, which for a file is not the order they play in.
   * The decoder only puts them right when it can see that a frame decoding
   * later presents sooner, and it reads that off these two: with both times
   * the same it has nothing to reorder by, and hands back what it was given. */
  CMTime decoded = CMTimeMake((int64_t)++vt->decoded, PTS_TIMESCALE);
  CMSampleTimingInfo timing = {CMTimeMake(1, PTS_TIMESCALE), at, decoded};
  CMSampleBufferRef sample = NULL;
  OSStatus made = CMSampleBufferCreate(kCFAllocatorDefault, block, true, NULL, NULL, vt->format, 1,
                                       1, &timing, 0, NULL, &sample);
  if (made != noErr) {
    CFRelease(block);
    return -1;
  }
  /* A live stream shows each picture as it comes and holds none back for
   * reordering; a file's come out in the order they play. */
  VTDecodeFrameFlags flags =
      vt->low_delay ? 0 : kVTDecodeFrame_EnableTemporalProcessing;
  OSStatus status = VTDecompressionSessionDecodeFrame(vt->session, sample, flags, vt, NULL);
  /* What the callback gave back waits for the caller, and this is what makes
   * it safe to write over the access unit the sample borrows. */
  settled(vt);
  CFRelease(sample);
  CFRelease(block);
  return status == noErr ? 0 : -1;
}

int yapfast_vt_next(struct yapfast_vt *vt, struct yapfast_vt_frame *frame, int64_t *out_pts_us) {
  if (!vt) return 0;
  *frame = (struct yapfast_vt_frame){0};
  if (out_pts_us) *out_pts_us = -1;
  return take(vt, frame, out_pts_us);
}

int yapfast_vt_flush(struct yapfast_vt *vt) {
  if (!vt || !vt->session) return 0;
  /* Pictures held for reordering only let go when told nothing more comes. */
  VTDecompressionSessionFinishDelayedFrames(vt->session);
  settled(vt);
  vt->flushed = 1;
  return 0;
}

void yapfast_vt_reset(struct yapfast_vt *vt) {
  if (!vt) return;
  /* What's held back was for the run of pictures that just ended. The session
   * stays: a file keeps its parameter sets to itself rather than repeating
   * them, so there'd be nothing to build another from, and a decoder starts
   * again on its own at the keyframe a seek lands on. */
  settled(vt);
  for (int index = 0; index < vt->pending_count; index++) CVBufferRelease(vt->pending[index]);
  vt->pending_count = 0;
  vt->flushed = 0;
  vt->decoded = 0;
}
