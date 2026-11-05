/* AAC on macOS through AudioToolbox, which is what the Mac's libwebrtc archive
 * leaves out: its FFmpeg decodes FLAC, MP3, Opus, PCM and Vorbis, and not AAC.
 *
 * An AudioConverter pulls packets through a callback until its output is full.
 * Answering "none" with zero packets and noErr would end the stream for good,
 * so the callback answers with a status of its own instead, which ends only
 * the call and keeps the decoder's state for the packets that follow. */
#include <stdlib.h>
#include <string.h>

#include <AudioToolbox/AudioConverter.h>

#include "audio_aac.h"

/* What `give_packet` says when nothing is waiting. */
static const OSStatus WAITING = 'wait';

/* How much waits for the converter at most, so a caller that queues without
 * decoding can't grow it without end. Every decode takes what's waiting, so a
 * few packets is what's ever in flight. */
enum { MOST_PACKETS = 32, MOST_BYTES = 16 * 1024 };

struct yapfast_aac {
  AudioConverterRef converter;
  int channels;
  /* The packets waiting, back to back, and where each one ends. */
  uint8_t bytes[MOST_BYTES];
  unsigned ends[MOST_PACKETS];
  unsigned count;
  /* How many of them the converter has taken during this decode. */
  unsigned taken;
  AudioStreamPacketDescription description;
};

static unsigned start_of(const struct yapfast_aac *aac, unsigned packet) {
  return packet ? aac->ends[packet - 1] : 0;
}

/* Gives the converter the next packet waiting, one per call. */
static OSStatus give_packet(AudioConverterRef converter, UInt32 *packets, AudioBufferList *list,
                            AudioStreamPacketDescription **descriptions, void *context) {
  (void)converter;
  struct yapfast_aac *aac = context;
  if (aac->taken == aac->count) {
    *packets = 0;
    return WAITING;
  }
  unsigned start = start_of(aac, aac->taken);
  unsigned size = aac->ends[aac->taken] - start;
  list->mNumberBuffers = 1;
  list->mBuffers[0].mNumberChannels = (UInt32)aac->channels;
  list->mBuffers[0].mData = aac->bytes + start;
  list->mBuffers[0].mDataByteSize = size;
  aac->description = (AudioStreamPacketDescription){.mDataByteSize = size};
  if (descriptions) *descriptions = &aac->description;
  aac->taken++;
  *packets = 1;
  return noErr;
}

struct yapfast_aac *yapfast_aac_new(double rate, int channels, const uint8_t *config,
                                    size_t config_size) {
  if (rate <= 0 || channels <= 0) return NULL;
  struct yapfast_aac *aac = calloc(1, sizeof *aac);
  if (!aac) return NULL;
  aac->channels = channels;
  AudioStreamBasicDescription source = {
      .mSampleRate = rate,
      .mFormatID = kAudioFormatMPEG4AAC,
      .mFramesPerPacket = 1024,
      .mChannelsPerFrame = (UInt32)channels,
  };
  /* Interleaved floats, which is what Yapfast keeps sound as. */
  AudioStreamBasicDescription samples = {
      .mSampleRate = rate,
      .mFormatID = kAudioFormatLinearPCM,
      .mFormatFlags = kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
      .mBitsPerChannel = 8 * (UInt32)sizeof(float),
      .mChannelsPerFrame = (UInt32)channels,
      .mFramesPerPacket = 1,
      .mBytesPerFrame = (UInt32)(sizeof(float) * channels),
      .mBytesPerPacket = (UInt32)(sizeof(float) * channels),
  };
  if (AudioConverterNew(&source, &samples, &aac->converter) != noErr) {
    free(aac);
    return NULL;
  }
  /* The file's AudioSpecificConfig. A refusal isn't fatal: the rate and
   * channels are enough for the decoder to get by on. */
  if (config && config_size) {
    AudioConverterSetProperty(aac->converter, kAudioConverterDecompressionMagicCookie,
                              (UInt32)config_size, config);
  }
  return aac;
}

int yapfast_aac_push(struct yapfast_aac *aac, const uint8_t *data, size_t size) {
  if (!aac || !data || size == 0) return -1;
  unsigned used = start_of(aac, aac->count);
  if (aac->count == MOST_PACKETS || used + size > MOST_BYTES) return -1;
  memcpy(aac->bytes + used, data, size);
  aac->ends[aac->count++] = used + (unsigned)size;
  return 0;
}

int yapfast_aac_next(struct yapfast_aac *aac, float *samples, int capacity) {
  if (!aac || !aac->count || !samples || capacity <= 0) return 0;
  UInt32 frames = (UInt32)capacity;
  AudioBufferList list = {
      .mNumberBuffers = 1,
      .mBuffers[0] =
          {
              .mNumberChannels = (UInt32)aac->channels,
              .mDataByteSize = frames * (UInt32)(sizeof(float) * aac->channels),
              .mData = samples,
          },
  };
  aac->taken = 0;
  OSStatus status =
      AudioConverterFillComplexBuffer(aac->converter, give_packet, aac, &frames, &list, NULL);
  /* What the converter took goes; a full output can leave some waiting. */
  unsigned gone = start_of(aac, aac->taken);
  unsigned left = aac->count - aac->taken;
  memmove(aac->bytes, aac->bytes + gone, start_of(aac, aac->count) - gone);
  for (unsigned packet = 0; packet < left; packet++) {
    aac->ends[packet] = aac->ends[aac->taken + packet] - gone;
  }
  aac->count = left;
  if (status != noErr && status != WAITING) return -1;
  return (int)frames;
}

void yapfast_aac_reset(struct yapfast_aac *aac) {
  if (!aac) return;
  aac->count = 0;
  if (aac->converter) AudioConverterReset(aac->converter);
}

void yapfast_aac_free(struct yapfast_aac *aac) {
  if (!aac) return;
  if (aac->converter) AudioConverterDispose(aac->converter);
  free(aac);
}
