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

/* The frames `yapfast_aac_next` is given room for: a few access units of 1024,
 * or 2048 with the bandwidth extension some files use. */
enum { YAPFAST_AAC_FRAMES = 4096 };

/* Queues one access unit. Returns a negative number when there's no room for
 * it, which is a packet to leave out rather than a stream to give up on. */
int yapfast_aac_push(struct yapfast_aac *aac, const uint8_t *data, size_t size);

/* Decodes what's queued into `samples`, which holds `capacity` frames of
 * `channels` interleaved floats. The decoder keeps its state from one call to
 * the next, as a stream's does. Returns how many frames it wrote, which may be
 * none while it's priming, or a negative number when it won't go on. */
int yapfast_aac_next(struct yapfast_aac *aac, float *samples, int capacity);

/* Forgets what's queued and what the decoder holds, as a seek needs. */
void yapfast_aac_reset(struct yapfast_aac *aac);

void yapfast_aac_free(struct yapfast_aac *aac);

#endif /* YAPFAST_AUDIO_AAC_H */
