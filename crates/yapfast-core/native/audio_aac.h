/* AAC on macOS, by AudioToolbox. The libwebrtc archive Yapfast links on the Mac
 * carries FFmpeg's audio decoders except AAC, which is the sound most video
 * files have, so without this an MP4 from a phone or a screen recording plays
 * silently. Only macOS builds it: see audio_aac.c. */
#ifndef YAPFAST_AUDIO_AAC_H
#define YAPFAST_AUDIO_AAC_H

#include <stddef.h>
#include <stdint.h>

struct yapfast_aac;

/* A decoder for AAC at `rate` with `channels` channels. `config` is a file's
 * AudioSpecificConfig, which says what the stream is beyond its rate and
 * channels; it may be NULL. Returns NULL when the system won't take the format. */
struct yapfast_aac *yapfast_aac_new(double rate, int channels, const uint8_t *config,
                                    size_t config_size);

/* How many frames one call to `yapfast_aac_next` can give back. An access unit
 * is 1024 frames, and one with the bandwidth extension some files use doubles
 * that, with room to spare. */
enum { YAPFAST_AAC_FRAMES = 4096 };

/* Queues one access unit, to come out with the ones already waiting. Returns a
 * negative number when there's no room for it, which is a packet to leave out
 * rather than a stream to give up on. */
int yapfast_aac_push(struct yapfast_aac *aac, const uint8_t *data, size_t size);

/* Decodes everything queued into `samples`, which holds `capacity` frames of
 * `channels` interleaved floats, and takes the packets it used. Returns how
 * many frames it wrote, which may be none when it's still working for more, or
 * a negative number when it won't go on. */
int yapfast_aac_next(struct yapfast_aac *aac, float *samples, int capacity);

/* Tells the converter to forget everything: what it's holding belongs to the
 * run of sound that just ended. A seek calls this. */
void yapfast_aac_reset(struct yapfast_aac *aac);

void yapfast_aac_free(struct yapfast_aac *aac);

#endif /* YAPFAST_AUDIO_AAC_H */
