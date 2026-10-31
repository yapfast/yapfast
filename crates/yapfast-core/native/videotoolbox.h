/* H264 decoding on macOS, by VideoToolbox. The libwebrtc archive Yapfast links
 * on the Mac carries FFmpeg's audio decoders and no video ones, so
 * avcodec_find_decoder(AV_CODEC_ID_H264) answers nothing there. This is what
 * answers instead. It hands out the same I420 planes FFmpeg did, so the Rust
 * side and the callers' own picture structs don't change. Only macOS builds it:
 * see videotoolbox.c. */
#ifndef YAPFAST_VIDEOTOOLBOX_H
#define YAPFAST_VIDEOTOOLBOX_H

#include <stddef.h>
#include <stdint.h>

/* A decoded picture as I420 planes, and how to read its colours. The planes
 * belong to the decoder and last until it decodes the next picture. */
struct yapfast_vt_frame {
  int width;
  int height;
  const uint8_t *data[3];
  int stride[3];
  /* Whether colours are BT.709: -1 when the picture doesn't say. */
  int bt709;
  /* Whether the luma and chroma run the full 0 to 255. */
  int full_range;
};

struct yapfast_vt;

/* A decoder. With `low_delay`, one access unit in and one picture out with no
 * reordering, which is what a live stream is; without it, pictures come out in
 * the order they play, which costs holding a few back. `threads` means nothing
 * here: the hardware takes the whole frame. */
struct yapfast_vt *yapfast_vt_new(int low_delay);

/* Primes the decoder with a file's H264Config, or the Annex-B some files carry
 * instead, so it can start before the first frame says what it is. Returns 0
 * when the decoder is ready, and a negative number when the data is not an
 * H264Config it can read. */
int yapfast_vt_set_extradata(struct yapfast_vt *vt, const uint8_t *data, size_t size);

/* Decodes one access unit: Annex-B when `avcc` is 0, or already prefixed with
 * each NAL unit's length when it is 1, as a video file stores them. `pts_us` is
 * when it plays, in microseconds from the start, or -1; it comes back from
 * `yapfast_vt_next` with the picture it belongs to.
 *
 * What comes out waits for `yapfast_vt_next`, because a file's pictures are
 * decoded out of the order they play. Returns 0 once the access unit is in, or
 * a negative number when it can't be decoded. */
int yapfast_vt_decode(struct yapfast_vt *vt, const uint8_t *data, size_t size, int avcc,
                      int64_t pts_us);

/* The next decoded picture, and when it plays. Returns 1 with one in `frame`
 * and its time in `*out_pts_us`, and 0 when the decoder is holding nothing. */
int yapfast_vt_next(struct yapfast_vt *vt, struct yapfast_vt_frame *frame, int64_t *out_pts_us);

/* Tells the decoder nothing more comes, so it lets go of the pictures it was
 * holding to reorder. After this, `yapfast_vt_next` gives up the rest. */
int yapfast_vt_flush(struct yapfast_vt *vt);

/* Tells the decoder to forget everything: what it's holding to reorder belongs
 * to what came before, and it's building from a keyframe again. A seek calls
 * this before it hands over the access units after its target. */
void yapfast_vt_reset(struct yapfast_vt *vt);

void yapfast_vt_free(struct yapfast_vt *vt);

#endif /* YAPFAST_VIDEOTOOLBOX_H */
