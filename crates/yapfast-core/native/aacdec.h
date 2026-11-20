/* AAC decoding on macOS through AudioToolbox, for video.c, where libwebrtc's
 * FFmpeg has no AAC decoder. A packet gives its samples as interleaved floats,
 * which stay valid until the next packet. */
#ifndef YAPFAST_AACDEC_H
#define YAPFAST_AACDEC_H

#include <stddef.h>
#include <stdint.h>

struct yapfast_aac;

/* A decoder for AAC of `channels` channels at `sample_rate`, with the file's
 * AudioSpecificConfig. NULL when AudioToolbox can't decode it. */
struct yapfast_aac *yapfast_aac_new(int sample_rate, int channels, const unsigned char *config,
                                    size_t config_size);
void yapfast_aac_free(struct yapfast_aac *aac);

/* Decodes one packet, dropping `skip_start` frames from its start and
 * `skip_end` from its end, as a file says of the encoder's padding. Returns
 * 0, or a negative number when it couldn't be decoded. */
int yapfast_aac_decode(struct yapfast_aac *aac, const unsigned char *data, size_t size,
                       int64_t pts, unsigned skip_start, unsigned skip_end);

/* Takes the samples of the last packet decoded: the float samples, or NULL
 * when there are none left. `frames` is how many of each channel, `pts` the
 * packet's time. */
const float *yapfast_aac_take(struct yapfast_aac *aac, int *frames, int64_t *pts);

/* Forgets what came before, as after a seek. */
void yapfast_aac_reset(struct yapfast_aac *aac);

#endif
