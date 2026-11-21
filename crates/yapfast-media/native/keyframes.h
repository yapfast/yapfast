#ifndef YAPFAST_KEYFRAMES_H
#define YAPFAST_KEYFRAMES_H

/* How long a stream goes without a keyframe nobody asked for, in seconds, for
 * every encoder here. Keyframes are made when they're asked for: a viewer who
 * joins, or whose picture broke, asks with a picture loss indication, and the
 * stream answers at once (forced IDR, or x264's keyframe on the next frame), as
 * libwebrtc's encoders do with no interval at all. This one is only the net
 * under a request that was lost.
 *
 * A keyframe is many times a frame: a 3440x1440 desktop's is about 450 kB,
 * which takes about 85 ms to pace out at a 20 Mbit/s stream's rate, and the
 * frames behind it fill the send queue until one is skipped. Every three
 * seconds, as this was, that was a hitch every three seconds. */
#define YAPFAST_KEYFRAME_SECONDS 30

#endif
