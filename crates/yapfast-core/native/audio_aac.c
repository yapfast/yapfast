/* AAC on macOS through AudioToolbox, which is what the Mac's libwebrtc archive
 * leaves out: its FFmpeg decodes FLAC, MP3, Opus, PCM and Vorbis, and not AAC.
 * AAC is the sound most video files have, so without this an MP4 from a phone,
 * or a screen recording, plays silently.
 *
 * An AudioConverter isn't a packet-in, samples-out decoder. It fills an output
 * buffer and asks for as many packets as it takes to fill it, through the
 * callback below, and once it's told there's nothing more it stops making
 * samples at all. So packets queue here and go over in runs: handing one at a
 * time and then saying there's no more leaves everything but the first packet
 * undecoded. */
#include <stdlib.h>
#include <string.h>

#include <AudioToolbox/AudioConverter.h>
#include <CoreAudio/AudioHardwareBase.h>
#include <CoreAudio/CoreAudioTypes.h>

#include "audio_aac.h"

/* How much waits for the converter at most. A packet is a few hundred bytes and
 * the converter takes a run of them every call, so this is well past what's
 * ever in flight; the room is bounded so a caller that queues without draining
 * can't grow without end. */
enum { MOST_PACKETS = 32, MOST_BYTES = 8 * 1024 };

struct yapfast_aac {
  AudioConverterRef converter;
  int channels;
  /* The packets waiting, back to back, and where each one starts. */
  uint8_t *bytes;
  unsigned size;
  unsigned starts[MOST_PACKETS];
  unsigned count;
  /* How many of them the converter has taken since it last asked for samples. */
  unsigned handed;
  /* What it's told about each one, filled in as they go over. */
  AudioStreamPacketDescription descriptions[MOST_PACKETS];
  AudioBufferList list;
};

/* Gives the converter every packet waiting. They're already back to back, so
 * it's one buffer and a word on where each packet begins. */
static OSStatus give_packets(AudioConverterRef converter, UInt32 *packets, AudioBufferList *list,
                             AudioStreamPacketDescription **descriptions, void *ref_con) {
  struct yapfast_aac *aac = ref_con;
  (void)converter;
  if (aac->handed == aac->count) {
    *packets = 0;
    return noErr;
  }
  unsigned first = aac->handed;
  unsigned from = aac->starts[first];
  list->mNumberBuffers = 1;
  list->mBuffers[0].mNumberChannels = (UInt32)aac->channels;
  list->mBuffers[0].mData = aac->bytes + from;
  list->mBuffers[0].mDataByteSize = aac->size - from;
  for (unsigned index = first; index < aac->count; index++) {
    unsigned start = aac->starts[index];
    unsigned end = index + 1 < aac->count ? aac->starts[index + 1] : aac->size;
    aac->descriptions[index].mStartOffset = (SInt64)(start - from);
    aac->descriptions[index].mVariableFramesInPacket = 0;
    aac->descriptions[index].mDataByteSize = end - start;
  }
  if (descriptions) *descriptions = aac->descriptions + first;
  aac->handed = aac->count;
  *packets = aac->count - first;
  return noErr;
}

struct yapfast_aac *yapfast_aac_new(double rate, int channels, const uint8_t *config,
                                   size_t config_size) {
  if (rate <= 0 || channels <= 0) return NULL;
  struct yapfast_aac *aac = calloc(1, sizeof *aac);
  if (!aac) return NULL;
  aac->channels = channels;
  aac->bytes = malloc(MOST_BYTES);
  if (!aac->bytes) {
    free(aac);
    return NULL;
  }
  aac->list.mNumberBuffers = 1;
  aac->list.mBuffers[0].mNumberChannels = (UInt32)channels;
  AudioStreamBasicDescription source = {
      .mSampleRate = rate,
      .mFormatID = kAudioFormatMPEG4AAC,
      .mFramesPerPacket = 1024,
      .mChannelsPerFrame = (UInt32)channels,
  };
  /* Interleaved floats, which is what Yapfast keeps sound as, so the samples
   * it gives back need nothing done to them. */
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
    free(aac->bytes);
    free(aac);
    return NULL;
  }
  /* What the stream is beyond its rate and channels. The system's decoder takes
   * it or doesn't and carries on either way, so a refusal isn't a reason to give
   * up: it has the rate, the channels and the packets to get by on. */
  if (config && config_size) {
    AudioConverterSetProperty(aac->converter, kAudioConverterDecompressionMagicCookie,
                              (UInt32)config_size, config);
  }
  return aac;
}

int yapfast_aac_push(struct yapfast_aac *aac, const uint8_t *data, size_t size) {
  if (!aac || !data || size == 0) return -1;
  if (aac->count == MOST_PACKETS || aac->size + size > MOST_BYTES) return -1;
  aac->starts[aac->count++] = aac->size;
  memcpy(aac->bytes + aac->size, data, size);
  aac->size += (unsigned)size;
  return 0;
}

int yapfast_aac_next(struct yapfast_aac *aac, float *samples, int capacity) {
  if (!aac || !aac->count || !samples || capacity <= 0) return 0;
  if (capacity > YAPFAST_AAC_FRAMES) capacity = YAPFAST_AAC_FRAMES;
  UInt32 frames = (UInt32)capacity;
  aac->list.mBuffers[0].mData = samples;
  aac->list.mBuffers[0].mDataByteSize = frames * (UInt32)sizeof(float) * (UInt32)aac->channels;
  aac->handed = 0;
  OSStatus status = AudioConverterFillComplexBuffer(aac->converter, give_packets, aac, &frames,
                                                   &aac->list, NULL);
  /* The converter took what it was offered, so none of it is waiting any more.
   * An error says nothing about how far it got, so the run goes rather than
   * risk turning part of it into samples twice; the next packets start clean. */
  aac->size = 0;
  aac->count = 0;
  if (status != noErr) return -1;
  return (int)frames;
}

void yapfast_aac_reset(struct yapfast_aac *aac) {
  if (!aac) return;
  /* The sound it's still working on was of the run that just ended, and the
   * packets waiting with it. */
  aac->size = 0;
  aac->count = 0;
  aac->handed = 0;
  if (aac->converter) AudioConverterReset(aac->converter);
}

void yapfast_aac_free(struct yapfast_aac *aac) {
  if (!aac) return;
  if (aac->converter) AudioConverterDispose(aac->converter);
  free(aac->bytes);
  free(aac);
}
