/* See aacdec.h. An AudioConverter turns AAC packets into float samples. */
#include "aacdec.h"

#include <AudioToolbox/AudioToolbox.h>
#include <stdlib.h>
#include <string.h>

/* The samples in an AAC packet; HE-AAC's sound twice as many, and no packet
 * gives more than this. */
enum { FRAMES_PER_PACKET = 1024, MAX_FRAMES = 4096 };

/* What the input callback answers when the packet it had has been taken: not
 * an error, as the converter has its samples by then. */
static const OSStatus NO_MORE_PACKETS = 'nmpk';

struct yapfast_aac {
  AudioConverterRef converter;
  int channels;
  float *samples;
  int frames;
  int64_t pts;
  /* The packet the converter is being given. */
  const unsigned char *packet;
  UInt32 packet_size;
  AudioStreamPacketDescription description;
};

/* The longest AudioSpecificConfig the descriptor below holds in one-byte sizes. */
enum { MAX_CONFIG = 100 };

/* MPEG-4 descriptor tags and values. */
enum {
  TAG_ES = 0x03,
  TAG_DECODER_CONFIG = 0x04,
  TAG_DECODER_SPECIFIC = 0x05,
  TAG_SL_CONFIG = 0x06,
  OBJECT_AAC = 0x40,
  STREAM_AUDIO = 0x15
};

/* AudioToolbox takes the config as an MPEG-4 elementary stream descriptor, as
 * an mp4's esds box holds it, rather than the bare AudioSpecificConfig. Returns
 * its size, in `descriptor`. */
static size_t wrap_config(const unsigned char *config, size_t size, unsigned char *descriptor) {
  size_t specific = 2 + size;
  size_t decoder = 13 + specific;
  /* The descriptor's own size: ES id and flags, the decoder config with its
   * header, and the SL config descriptor. */
  size_t total = 3 + 2 + decoder + 3;
  unsigned char *at = descriptor;
  *at++ = TAG_ES;
  *at++ = (unsigned char)total;
  *at++ = 0;
  *at++ = 0;
  *at++ = 0;
  *at++ = TAG_DECODER_CONFIG;
  *at++ = (unsigned char)decoder;
  *at++ = OBJECT_AAC;
  *at++ = STREAM_AUDIO;
  memset(at, 0, 11);
  at += 11;
  *at++ = TAG_DECODER_SPECIFIC;
  *at++ = (unsigned char)size;
  memcpy(at, config, size);
  at += size;
  *at++ = TAG_SL_CONFIG;
  *at++ = 1;
  *at++ = 2;
  return (size_t)(at - descriptor);
}

struct yapfast_aac *yapfast_aac_new(int sample_rate, int channels, const unsigned char *config,
                                    size_t config_size) {
  if (sample_rate <= 0 || channels <= 0 || channels > 8) return NULL;
  struct yapfast_aac *aac = calloc(1, sizeof *aac);
  if (!aac) return NULL;
  aac->channels = channels;
  aac->samples = malloc((size_t)MAX_FRAMES * (size_t)channels * sizeof(float));
  AudioStreamBasicDescription coded = {.mSampleRate = sample_rate,
                                       .mFormatID = kAudioFormatMPEG4AAC,
                                       .mFramesPerPacket = FRAMES_PER_PACKET,
                                       .mChannelsPerFrame = (UInt32)channels};
  AudioStreamBasicDescription pcm = {.mSampleRate = sample_rate,
                                     .mFormatID = kAudioFormatLinearPCM,
                                     .mFormatFlags = kAudioFormatFlagsNativeFloatPacked,
                                     .mBytesPerPacket = (UInt32)channels * sizeof(float),
                                     .mFramesPerPacket = 1,
                                     .mBytesPerFrame = (UInt32)channels * sizeof(float),
                                     .mChannelsPerFrame = (UInt32)channels,
                                     .mBitsPerChannel = 8 * sizeof(float)};
  if (!aac->samples || AudioConverterNew(&coded, &pcm, &aac->converter) != noErr) {
    yapfast_aac_free(aac);
    return NULL;
  }
  if (config_size > MAX_CONFIG) {
    yapfast_aac_free(aac);
    return NULL;
  }
  if (config_size > 0) {
    unsigned char descriptor[MAX_CONFIG + 32];
    size_t descriptor_size = wrap_config(config, config_size, descriptor);
    if (AudioConverterSetProperty(aac->converter, kAudioConverterDecompressionMagicCookie,
                                  (UInt32)descriptor_size, descriptor) != noErr) {
      yapfast_aac_free(aac);
      return NULL;
    }
  }
  return aac;
}

void yapfast_aac_free(struct yapfast_aac *aac) {
  if (!aac) return;
  if (aac->converter) AudioConverterDispose(aac->converter);
  free(aac->samples);
  free(aac);
}

/* The converter's way to ask for its next packet: the one held, once. */
static OSStatus give_packet(AudioConverterRef converter, UInt32 *packets, AudioBufferList *data,
                            AudioStreamPacketDescription **description, void *refcon) {
  (void)converter;
  struct yapfast_aac *aac = refcon;
  if (!aac->packet) {
    *packets = 0;
    return NO_MORE_PACKETS;
  }
  aac->description = (AudioStreamPacketDescription){.mDataByteSize = aac->packet_size};
  data->mNumberBuffers = 1;
  data->mBuffers[0].mNumberChannels = (UInt32)aac->channels;
  data->mBuffers[0].mDataByteSize = aac->packet_size;
  data->mBuffers[0].mData = (void *)aac->packet;
  if (description) *description = &aac->description;
  *packets = 1;
  aac->packet = NULL;
  return noErr;
}

/* Keeps the `frames` decoded but `skip_start` at the front and `skip_end` at
 * the back. */
static void trim(struct yapfast_aac *aac, UInt32 frames, unsigned skip_start, unsigned skip_end) {
  if ((UInt64)skip_start + skip_end >= frames) {
    aac->frames = 0;
    return;
  }
  if (skip_start > 0) {
    memmove(aac->samples, aac->samples + (size_t)skip_start * (size_t)aac->channels,
            (size_t)(frames - skip_start) * (size_t)aac->channels * sizeof(float));
  }
  aac->frames = (int)(frames - skip_start - skip_end);
}

int yapfast_aac_decode(struct yapfast_aac *aac, const unsigned char *data, size_t size,
                       int64_t pts, unsigned skip_start, unsigned skip_end) {
  aac->frames = 0;
  if (size == 0 || size > UINT32_MAX) return -1;
  aac->packet = data;
  aac->packet_size = (UInt32)size;
  AudioBufferList output = {.mNumberBuffers = 1};
  output.mBuffers[0].mNumberChannels = (UInt32)aac->channels;
  output.mBuffers[0].mDataByteSize = (UInt32)((size_t)MAX_FRAMES * (size_t)aac->channels * sizeof(float));
  output.mBuffers[0].mData = aac->samples;
  UInt32 frames = MAX_FRAMES;
  OSStatus status = AudioConverterFillComplexBuffer(aac->converter, give_packet, aac, &frames,
                                                    &output, NULL);
  aac->packet = NULL;
  if (status != noErr && status != NO_MORE_PACKETS) return -1;
  trim(aac, frames, skip_start, skip_end);
  aac->pts = pts;
  return 0;
}

const float *yapfast_aac_take(struct yapfast_aac *aac, int *frames, int64_t *pts) {
  if (aac->frames <= 0) return NULL;
  *frames = aac->frames;
  *pts = aac->pts;
  aac->frames = 0;
  return aac->samples;
}

void yapfast_aac_reset(struct yapfast_aac *aac) {
  aac->frames = 0;
  AudioConverterReset(aac->converter);
}
