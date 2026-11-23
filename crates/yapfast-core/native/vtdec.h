/* H264 decoding on macOS through VideoToolbox, for decode.c (a watched Go Live
 * stream, in Annex B form) and video.c (a file's packets, length prefixed as
 * the file's avcC says), where libwebrtc's FFmpeg has no H264 decoder. Pictures
 * come out as I420 planes that stay valid until the next call that gives one. */
#ifndef YAPFAST_VTDEC_H
#define YAPFAST_VTDEC_H

#include <stddef.h>
#include <stdint.h>

/* Used as a time when a packet has none. */
#define YAPFAST_VT_NO_TIME INT64_MIN

struct yapfast_vt;

struct yapfast_vt_picture {
  int width;
  int height;
  const unsigned char *y;
  const unsigned char *u;
  const unsigned char *v;
  int y_stride;
  int u_stride;
  int v_stride;
  /* The picture's time as given with its packet, or YAPFAST_VT_NO_TIME. */
  int64_t pts;
  /* Whether the stream says BT.709 (1), BT.601 (0) or nothing (-1), and
   * whether its range is full. */
  int bt709;
  int full_range;
};

struct yapfast_vt *yapfast_vt_new(void);
void yapfast_vt_free(struct yapfast_vt *vt);

/* Takes the parameter sets and NAL length size of a file's avcC. Returns 0, or
 * a negative number when it can't be read. */
int yapfast_vt_set_avcc(struct yapfast_vt *vt, const unsigned char *avcc, size_t size);

/* Decodes one access unit in Annex B form, its parameter sets in front of a
 * keyframe. Returns 0, or a negative number when it couldn't be decoded. */
int yapfast_vt_decode_annexb(struct yapfast_vt *vt, const unsigned char *data, size_t size,
                             int64_t pts, int64_t dts);

/* Decodes one packet of length prefixed NAL units, after yapfast_vt_set_avcc. */
int yapfast_vt_decode_avcc(struct yapfast_vt *vt, const unsigned char *data, size_t size,
                           int64_t pts, int64_t dts);

/* Gives the next picture in display order into `picture`: 1 with one, 0 when
 * none can be shown yet. After yapfast_vt_drain, all that are left come. */
int yapfast_vt_next(struct yapfast_vt *vt, struct yapfast_vt_picture *picture);

/* No more packets come: pictures held for reordering may be shown. */
void yapfast_vt_drain(struct yapfast_vt *vt);

/* Forgets the pictures held and waits for the stream to start again at a
 * keyframe, as after a seek. */
void yapfast_vt_reset(struct yapfast_vt *vt);

#endif
