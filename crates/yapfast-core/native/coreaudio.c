// Yapfast's sound on macOS: speakers and a microphone through CoreAudio's
// output unit, which is what the system gives an application that plays and
// records with no sound server in between.
//
// Each stream keeps a lock-free ring between Yapfast's own thread and the
// unit's, which takes and gives frames on the device's clock, so the unit's
// callback copies and does nothing else. The speakers' unit converts Yapfast's
// samples, floats or 16-bit, to the device's rate and format itself. The
// microphone's gives floats at the device's own rate, and Yapfast's thread
// converts those to what was asked for.
//
// Echo cancellation is WebRTC's, fed from Yapfast's own playback, so this uses
// the plain output unit rather than Apple's voice-processing one.

#include <AudioToolbox/AudioToolbox.h>
#include <CoreAudio/CoreAudio.h>
#include <dispatch/dispatch.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define YAPOK 0
#define YAPERR 1

/// Says why a call failed, into the buffer Rust gave.
static void fail(char *err, size_t length, const char *why) {
  if (err != NULL && length > 0) {
    snprintf(err, length, "%s", why);
  }
}

/// Why a call failed, with the four-letter code CoreAudio reports.
static void fail_status(char *err, size_t length, const char *what, OSStatus status) {
  char code[5] = {0, 0, 0, 0, 0};
  for (int at = 0; at < 4; at++) {
    char letter = (char)((status >> (24 - 8 * at)) & 0xff);
    code[at] = (letter >= 32 && letter < 127) ? letter : '?';
  }
  if (err != NULL && length > 0) {
    snprintf(err, length, "%s (%s)", what, code);
  }
}

// --- the ring between Yapfast's thread and the unit's ---

typedef struct {
  uint8_t *data;
  size_t size;
  /// The size of one frame, so the writer never stores part of one. A partial
  /// frame leaves `written` off a frame boundary, and every read after it
  /// returns the samples shifted by those bytes, which is noise and NaN.
  size_t align;
  /// One thread writes and one reads, so each moves only its own position and
  /// neither takes a lock: the unit's thread never waits on Yapfast's. The
  /// positions only grow, and wrap into the buffer by masking.
  _Atomic size_t written;
  _Atomic size_t read;
  /// Yapfast's side sleeps on `moved` when it has to wait, and says so in
  /// `waiting`, so the unit's thread signals only then, not every callback.
  dispatch_semaphore_t moved;
  atomic_bool waiting;
  atomic_bool closed;
} Ring;

/// At least 32 KiB, and twice what a stream asked for, rounded up to a power of
/// two so the indexes wrap by masking.
static size_t ring_size(size_t bytes) {
  size_t size = 1u << 15;
  while (size < bytes * 2) {
    size <<= 1;
  }
  return size;
}

static int ring_open(Ring *ring, size_t bytes, size_t align) {
  memset(ring, 0, sizeof(*ring));
  ring->size = ring_size(bytes);
  ring->align = align > 0 ? align : 1;
  ring->data = calloc(ring->size, 1);
  ring->moved = dispatch_semaphore_create(0);
  if (ring->data == NULL || ring->moved == NULL) {
    return YAPERR;
  }
  return YAPOK;
}

/// After the unit has stopped, so its thread no longer touches the ring.
static void ring_free(Ring *ring) {
  atomic_store(&ring->closed, true);
  if (ring->moved != NULL) {
    dispatch_semaphore_signal(ring->moved);
    dispatch_release(ring->moved);
    ring->moved = NULL;
  }
  free(ring->data);
  ring->data = NULL;
}

static size_t ring_used(Ring *ring) { return atomic_load(&ring->written) - atomic_load(&ring->read); }

/// The unit's thread, after it moved the ring: wakes Yapfast's if it waits.
static void ring_moved(Ring *ring) {
  if (atomic_exchange(&ring->waiting, false)) {
    dispatch_semaphore_signal(ring->moved);
  }
}

/// Copies out up to `length` bytes without waiting.
static size_t ring_take(Ring *ring, uint8_t *out, size_t length) {
  size_t read = atomic_load_explicit(&ring->read, memory_order_relaxed);
  size_t got = atomic_load_explicit(&ring->written, memory_order_acquire) - read;
  if (got > length) {
    got = length;
  }
  size_t at = read & (ring->size - 1);
  size_t head = ring->size - at;
  if (head > got) {
    head = got;
  }
  memcpy(out, ring->data + at, head);
  memcpy(out + head, ring->data, got - head);
  atomic_store_explicit(&ring->read, read + got, memory_order_release);
  return got;
}

/// Copies in up to `length` bytes without waiting, dropping what won't fit, in
/// whole frames, so the read position stays on a frame boundary.
static size_t ring_give(Ring *ring, const uint8_t *in, size_t length) {
  size_t written = atomic_load_explicit(&ring->written, memory_order_relaxed);
  size_t put = ring->size - (written - atomic_load_explicit(&ring->read, memory_order_acquire));
  if (put > length) {
    put = length;
  }
  put -= put % ring->align;
  size_t at = written & (ring->size - 1);
  size_t head = ring->size - at;
  if (head > put) {
    head = put;
  }
  memcpy(ring->data + at, in, head);
  memcpy(ring->data, in + head, put - head);
  atomic_store_explicit(&ring->written, written + put, memory_order_release);
  return put;
}

/// What Yapfast's side waits for.
enum Until { FILLED, ROOM, EMPTY };

static bool ring_ready(Ring *ring, enum Until until, size_t bytes) {
  size_t used = ring_used(ring);
  switch (until) {
    case FILLED:
      return used >= bytes;
    case ROOM:
      return used < ring->size;
    case EMPTY:
      return used == 0;
  }
  return true;
}

/// Sleeps until `until` holds; false if the ring closed first. The timeout
/// keeps a unit that stops calling back from holding Yapfast for ever.
static bool ring_await(Ring *ring, enum Until until, size_t bytes) {
  while (!ring_ready(ring, until, bytes)) {
    if (atomic_load(&ring->closed)) {
      return false;
    }
    // Said before looking again, so a move between the two still wakes it.
    atomic_store(&ring->waiting, true);
    if (ring_ready(ring, until, bytes)) {
      break;
    }
    dispatch_semaphore_wait(ring->moved, dispatch_time(DISPATCH_TIME_NOW, 20 * NSEC_PER_MSEC));
  }
  return true;
}

/// Waits for room and then takes every byte, which is how Yapfast queues sound
/// to play: it blocks while the speakers still have a full queue ahead of it.
static int ring_fill(Ring *ring, const uint8_t *in, size_t length, char *err, size_t err_length) {
  for (size_t done = 0; done < length;) {
    if (!ring_await(ring, ROOM, 0)) {
      fail(err, err_length, "the stream is closed");
      return YAPERR;
    }
    done += ring_give(ring, in + done, length - done);
  }
  return YAPOK;
}

/// Sleeps out the device's own delay after a drain, so sound queued behind what
/// was drained has had its turn.
static void wait_us(uint64_t microseconds) {
  struct timespec remaining = {
      .tv_sec = (time_t)(microseconds / 1000000),
      .tv_nsec = (long)((microseconds % 1000000) * 1000),
  };
  while (nanosleep(&remaining, &remaining) == -1 && remaining.tv_nsec > 0) {
  }
}

// --- devices ---

/// Every device CoreAudio knows about.
static size_t devices(AudioDeviceID *found, size_t room) {
  AudioObjectPropertyAddress address = {
      kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain};
  UInt32 size = 0;
  if (AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &address, 0, NULL, &size) != noErr) {
    return 0;
  }
  size_t count = size / sizeof(AudioDeviceID);
  if (found == NULL || count == 0) {
    return count;
  }
  if (count > room) {
    count = room;
  }
  size = (UInt32)(count * sizeof(AudioDeviceID));
  if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, 0, NULL, &size, found) != noErr) {
    return 0;
  }
  return count;
}

/// How many channels a device gives or takes.
static uint32_t channel_count(AudioDeviceID device, AudioObjectPropertyScope scope) {
  AudioObjectPropertyAddress address = {kAudioDevicePropertyStreamConfiguration, scope,
                                        kAudioObjectPropertyElementMain};
  UInt32 size = 0;
  if (AudioObjectGetPropertyDataSize(device, &address, 0, NULL, &size) != noErr || size == 0) {
    return 0;
  }
  AudioBufferList *list = malloc(size);
  if (list == NULL) {
    return 0;
  }
  uint32_t channels = 0;
  if (AudioObjectGetPropertyData(device, &address, 0, NULL, &size, list) == noErr) {
    for (UInt32 buffer = 0; buffer < list->mNumberBuffers; buffer++) {
      channels += list->mBuffers[buffer].mNumberChannels;
    }
  }
  free(list);
  return channels;
}

/// The name CoreAudio shows for a device, into `out`.
static bool device_name(AudioDeviceID device, char *out, size_t length) {
  AudioObjectPropertyAddress address = {kAudioDevicePropertyDeviceNameCFString,
                                        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain};
  CFStringRef name = NULL;
  UInt32 size = sizeof(name);
  if (AudioObjectGetPropertyData(device, &address, 0, NULL, &size, &name) != noErr || name == NULL) {
    return false;
  }
  bool read = CFStringGetCString(name, out, (CFIndex)length, kCFStringEncodingUTF8);
  CFRelease(name);
  return read;
}

/// The device an id from Yapfast's settings is: the system default for none, a
/// device's number, or a device's name, which is how WebRTC names the macOS
/// devices it lists, and what `voice::device_id` then keeps as the id.
static AudioDeviceID resolve(const char *id, bool output, char *err, size_t err_length) {
  AudioObjectPropertyScope scope =
      output ? kAudioObjectPropertyScopeOutput : kAudioObjectPropertyScopeInput;
  if (id == NULL || id[0] == 0) {
    AudioObjectPropertyAddress address = {
        output ? kAudioHardwarePropertyDefaultOutputDevice : kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain};
    AudioDeviceID device = kAudioObjectUnknown;
    UInt32 size = sizeof(device);
    if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, 0, NULL, &size, &device) != noErr ||
        device == kAudioObjectUnknown) {
      fail(err, err_length, output ? "no default speakers" : "no default microphone");
      return kAudioObjectUnknown;
    }
    return device;
  }

  AudioDeviceID found[64];
  size_t count = devices(found, 64);

  // A plain number is a device's own, which is what CoreAudio calls one.
  char *rest = NULL;
  unsigned long as_number = strtoul(id, &rest, 10);
  if (rest != id && *rest == 0) {
    for (size_t at = 0; at < count; at++) {
      if (found[at] == (AudioDeviceID)as_number && channel_count(found[at], scope) > 0) {
        return found[at];
      }
    }
  }

  // Settings keep a device with no identifier of its own as "name:" and its
  // name; WebRTC's macOS names come from the same CoreAudio string.
  const char *wanted = id;
  if (strncmp(wanted, "name:", 5) == 0) {
    wanted += 5;
  }
  char name[256];
  for (size_t at = 0; at < count; at++) {
    if (channel_count(found[at], scope) == 0 || !device_name(found[at], name, sizeof(name))) {
      continue;
    }
    if (strcmp(name, wanted) == 0) {
      return found[at];
    }
  }
  fail(err, err_length, output ? "those speakers are no longer connected"
                               : "that microphone is no longer connected");
  return kAudioObjectUnknown;
}

/// The format to ask a unit for: 32-bit floats as they sit in memory, which is
/// what both of Yapfast's rings hold.
/// Interleaved floats, or 16-bit integers.
static AudioStreamBasicDescription sample_format(uint32_t rate, uint32_t channels, bool floats) {
  uint32_t bytes = floats ? 4 : 2;
  return (AudioStreamBasicDescription){
      .mFormatID = kAudioFormatLinearPCM,
      .mFormatFlags = (floats ? kAudioFormatFlagIsFloat : kAudioFormatFlagIsSignedInteger) |
                      kAudioFormatFlagIsPacked,
      .mSampleRate = rate,
      .mBitsPerChannel = 8 * bytes,
      .mChannelsPerFrame = channels,
      .mFramesPerPacket = 1,
      .mBytesPerFrame = bytes * channels,
      // A packet is a frame. Left at zero, the unit calls this format unsupported.
      .mBytesPerPacket = bytes * channels,
  };
}

static AudioStreamBasicDescription client_format(uint32_t rate, uint32_t channels) {
  return sample_format(rate, channels, true);
}

/// The block the device works in, so a ring knows how much to hold around the
/// queue Yapfast asked for.
static uint32_t io_frames(AudioDeviceID device) {
  AudioObjectPropertyAddress address = {kAudioDevicePropertyBufferFrameSize, kAudioObjectPropertyScopeGlobal,
                                        kAudioObjectPropertyElementMain};
  UInt32 frames = 0;
  UInt32 size = sizeof(frames);
  if (device == kAudioObjectUnknown ||
      AudioObjectGetPropertyData(device, &address, 0, NULL, &size, &frames) != noErr ||
      frames == 0) {
    return 512;
  }
  return frames;
}

/// Asks a device to run at `rate`, which is how a device that won't take Yapfast's
/// 48 kHz as a client rate is still made to give it. Every built-in one does.
// --- speakers ---

struct YAPOutput {
  AudioUnit unit;
  AudioDeviceID device;
  Ring ring;
  uint32_t rate;
  uint32_t channels;
  size_t frame_bytes;
  bool float_samples;
  bool started;
};

static uint64_t output_latency(struct YAPOutput *out) {
  uint64_t frames = 0;
  AudioObjectPropertyAddress address = {0, kAudioObjectPropertyScopeOutput, kAudioObjectPropertyElementMain};
  UInt32 value = 0;
  UInt32 size = sizeof(value);
  address.mSelector = kAudioDevicePropertyLatency;
  if (AudioObjectGetPropertyData(out->device, &address, 0, NULL, &size, &value) == noErr) {
    frames += value;
  }
  size = sizeof(value);
  address.mSelector = kAudioDevicePropertySafetyOffset;
  if (AudioObjectGetPropertyData(out->device, &address, 0, NULL, &size, &value) == noErr) {
    frames += value;
  }
  frames += io_frames(out->device);
  // What Yapfast has queued behind all of that.
  frames += ring_used(&out->ring) / out->frame_bytes;
  return frames * 1000000 / out->rate;
}

/// The unit's own thread: take what Yapfast queued, and stay quiet for the rest.
static OSStatus play_render(void *context, AudioUnitRenderActionFlags *flags,
                            const AudioTimeStamp *stamp, UInt32 bus, UInt32 frames,
                            AudioBufferList *audio) {
  (void)flags;
  (void)stamp;
  (void)bus;
  struct YAPOutput *out = context;
  if (audio->mNumberBuffers == 0 || audio->mBuffers[0].mData == NULL) {
    return noErr;
  }
  // The format asked for is packed, so one buffer holds every channel.
  AudioBuffer *buffer = &audio->mBuffers[0];
  size_t wanted = (size_t)frames * out->frame_bytes;
  if (wanted > buffer->mDataByteSize) {
    wanted = buffer->mDataByteSize;
  }
  size_t got = ring_take(&out->ring, buffer->mData, wanted);
  ring_moved(&out->ring);
  // Silence for whatever the ring didn't hold.
  memset((uint8_t *)buffer->mData + got, 0, buffer->mDataByteSize - got);
  return noErr;
}

void yap_output_free(struct YAPOutput *out);

struct YAPOutput *yap_output_open(const char *device_id, uint32_t rate, uint32_t channels,
                                  int float_samples, uint32_t ring_frames, char *err, size_t err_length) {
  struct YAPOutput *out = calloc(1, sizeof(*out));
  if (out == NULL) {
    fail(err, err_length, "out of memory");
    return NULL;
  }
  out->rate = rate;
  out->channels = channels;
  // The unit converts either kind of sample to the device's own format.
  out->float_samples = float_samples != 0;
  out->frame_bytes = (out->float_samples ? 4 : 2) * channels;

  AudioDeviceID device = resolve(device_id, true, err, err_length);
  if (device == kAudioObjectUnknown) {
    free(out);
    return NULL;
  }
  out->device = device;

  AudioComponentDescription description = {
      // The generic HAL unit, since Yapfast names the device it wants: the
      // default one is resolved by `resolve` before this, never left to the
      // unit's own subtype.
      .componentType = kAudioUnitType_Output,
      .componentSubType = kAudioUnitSubType_HALOutput,
      .componentManufacturer = kAudioUnitManufacturer_Apple,
      .componentFlags = 0,
      .componentFlagsMask = 0,
  };
  AudioComponent component = AudioComponentFindNext(NULL, &description);
  if (component == NULL) {
    fail(err, err_length, "no output unit");
    free(out);
    return NULL;
  }
  OSStatus status = AudioComponentInstanceNew(component, &out->unit);
  if (status != noErr) {
    fail_status(err, err_length, "could not make an output unit", status);
    free(out);
    return NULL;
  }
  status = AudioUnitSetProperty(out->unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0,
                                &device, sizeof(device));
  if (status != noErr) {
    fail_status(err, err_length, "those speakers are no longer connected", status);
    goto failed;
  }

  // The output unit resamples to the device itself, whatever its rate.
  AudioStreamBasicDescription format = sample_format(rate, channels, out->float_samples);
  status = AudioUnitSetProperty(out->unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 0,
                                &format, sizeof(format));
  if (status != noErr) {
    fail_status(err, err_length, "those speakers will not take this format", status);
    goto failed;
  }

  // The queue asked for, plus four blocks either side so the unit never waits
  // on Yapfast and Yapfast rarely waits on the unit.
  size_t bytes = (size_t)ring_frames * out->frame_bytes;
  if (bytes == 0) {
    bytes = (size_t)(rate / 10) * out->frame_bytes;
  }
  if (ring_open(&out->ring, bytes + (size_t)io_frames(device) * out->frame_bytes * 4, out->frame_bytes) != YAPOK) {
    fail(err, err_length, "out of memory");
    goto failed;
  }

  AURenderCallbackStruct callback = {play_render, out};
  status = AudioUnitSetProperty(out->unit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, 0,
                                &callback, sizeof(callback));
  if (status != noErr) {
    fail_status(err, err_length, "could not ask the unit for sound", status);
    goto failed;
  }
  status = AudioUnitInitialize(out->unit);
  if (status != noErr) {
    fail_status(err, err_length, "could not start those speakers", status);
    goto failed;
  }
  // The unit starts with the first sound and stops when the sound does, as
  // WASAPI's does: a running unit wakes the device every few milliseconds
  // even to play silence, and a paused video would keep it doing so.
  return out;

failed:
  // Whatever was set up before the failure goes too: a device that won't open
  // is tried again every half second.
  yap_output_free(out);
  return NULL;
}

/// Starts the unit if it isn't running.
static int start_output(struct YAPOutput *out, char *err, size_t err_length) {
  if (out->started) {
    return YAPOK;
  }
  OSStatus status = AudioOutputUnitStart(out->unit);
  if (status != noErr) {
    fail_status(err, err_length, "could not play on those speakers", status);
    return YAPERR;
  }
  out->started = true;
  return YAPOK;
}

static void stop_output(struct YAPOutput *out) {
  if (out->started) {
    AudioOutputUnitStop(out->unit);
    out->started = false;
  }
}

int yap_output_write(struct YAPOutput *out, const uint8_t *data, size_t bytes, char *err,
                     size_t err_length) {
  if (start_output(out, err, err_length) != YAPOK) {
    return YAPERR;
  }
  return ring_fill(&out->ring, data, bytes, err, err_length);
}

void yap_output_flush(struct YAPOutput *out) {
  // Stopped first, so the unit's thread isn't reading as the queue goes.
  stop_output(out);
  atomic_store(&out->ring.read, atomic_load(&out->ring.written));
}

int yap_output_drain(struct YAPOutput *out, char *err, size_t err_length) {
  ring_await(&out->ring, EMPTY, 0);
  wait_us(output_latency(out));
  stop_output(out);
  (void)err;
  (void)err_length;
  return YAPOK;
}

uint64_t yap_output_latency_us(struct YAPOutput *out) { return output_latency(out); }

void yap_output_free(struct YAPOutput *out) {
  if (out == NULL) {
    return;
  }
  stop_output(out);
  ring_free(&out->ring);
  if (out->unit != NULL) {
    AudioUnitUninitialize(out->unit);
    AudioComponentInstanceDispose(out->unit);
  }
  free(out);
}

// --- a microphone ---

struct YAPInput {
  AudioUnit unit;
  AudioDeviceID device;
  Ring ring;
  uint32_t rate;
  uint32_t channels;
  size_t frame_bytes;
  bool float_samples;
  uint8_t *scratch;
  size_t scratch_size;
  /// What the unit renders the microphone into, and how many frames at a time.
  uint8_t *recorded;
  UInt32 recorded_frames;
  /// What the unit really gives, read back after it was initialised: the
  /// microphone's own channel count, at whatever rate it settled on. A built-in
  /// microphone is mono, however stereo the client format was asked for, and
  /// the unit writes only that much, so everything downstream is measured in
  /// this rather than in what Yapfast hoped for.
  uint32_t render_channels;
  size_t render_frame_bytes;
  uint32_t render_rate;
  /// Floats at `render_channels`, then at `channels`, waiting to be resampled.
  float *mapped;
  size_t mapped_frames;
  float *resampled;
  size_t resampled_frames;
  /// Only when the device settled on another rate than Yapfast asked for.
  AudioConverterRef converter;
  bool started;
};

/// Reads the microphone's own format, which is the only honest answer to how
/// many channels there are.
static AudioStreamBasicDescription heard_format(AudioDeviceID device) {
  AudioStreamBasicDescription format;
  memset(&format, 0, sizeof(format));
  AudioObjectPropertyAddress address = {kAudioDevicePropertyStreamFormat, kAudioObjectPropertyScopeInput,
                                        kAudioObjectPropertyElementMain};
  UInt32 size = sizeof(format);
  if (AudioObjectGetPropertyData(device, &address, 0, NULL, &size, &format) != noErr) {
    memset(&format, 0, sizeof(format));
  }
  return format;
}

/// The unit's own thread. It hands over nothing: the microphone is pulled with
/// AudioUnitRender, into a list of Yapfast's own, and then dropped into the
/// ring, where a reader that has fallen far behind loses sound rather than
/// holds the device.
static OSStatus record_input(void *context, AudioUnitRenderActionFlags *flags,
                             const AudioTimeStamp *stamp, UInt32 bus, UInt32 frames,
                             AudioBufferList *audio) {
  (void)bus;
  (void)audio; // null for the microphone: this callback only says frames are ready
  struct YAPInput *in = context;
  if (in->recorded == NULL || frames == 0) {
    return noErr;
  }
  if (frames > in->recorded_frames) {
    frames = in->recorded_frames;
  }
  size_t bytes = frames * in->render_frame_bytes;
  AudioBufferList list;
  // The buffers are sized to what the unit renders, not to what was asked for:
  // sizing them to the request is how a mono microphone's second channel ends
  // up in the ring as whatever that memory happened to hold.
  list.mNumberBuffers = 1;
  list.mBuffers[0].mData = in->recorded;
  list.mBuffers[0].mNumberChannels = (UInt32)in->render_channels;
  list.mBuffers[0].mDataByteSize = (UInt32)bytes;
  // A failed render gives back nothing, so the silence written here is what
  // reaches the uplink rather than the last block twice over.
  memset(in->recorded, 0, bytes);
  AudioUnitRender(in->unit, flags, stamp, 1, frames, &list);
  ring_give(&in->ring, in->recorded, bytes);
  ring_moved(&in->ring);
  return noErr;
}

void yap_input_free(struct YAPInput *in);

struct YAPInput *yap_input_open(const char *device_id, uint32_t rate, uint32_t channels,
                                int float_samples, uint32_t ring_frames, char *err, size_t err_length) {
  struct YAPInput *in = calloc(1, sizeof(*in));
  if (in == NULL) {
    fail(err, err_length, "out of memory");
    return NULL;
  }
  in->rate = rate;
  in->channels = channels;
  in->float_samples = float_samples != 0;
  in->frame_bytes = 4 * channels;

  AudioDeviceID device = resolve(device_id, false, err, err_length);
  if (device == kAudioObjectUnknown) {
    free(in);
    return NULL;
  }
  in->device = device;

  AudioComponentDescription description = {
      // The same unit type records: CoreAudio has no input-only subtype, so
      // this turns the input side of the HAL unit on and leaves its output off.
      .componentType = kAudioUnitType_Output,
      .componentSubType = kAudioUnitSubType_HALOutput,
      .componentManufacturer = kAudioUnitManufacturer_Apple,
      .componentFlags = 0,
      .componentFlagsMask = 0,
  };
  AudioComponent component = AudioComponentFindNext(NULL, &description);
  if (component == NULL) {
    fail(err, err_length, "no input unit");
    free(in);
    return NULL;
  }
  OSStatus status = AudioComponentInstanceNew(component, &in->unit);
  if (status != noErr) {
    fail_status(err, err_length, "could not make an input unit", status);
    free(in);
    return NULL;
  }
  // The speakers on this unit go off first. An input-only device, which the
  // Apple silicon machines' microphone is, is otherwise refused below.
  UInt32 off = 0;
  status = AudioUnitSetProperty(in->unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, 0,
                                &off, sizeof(off));
  if (status != noErr) {
    fail_status(err, err_length, "this unit cannot record only", status);
    goto failed;
  }
  UInt32 enabled = 1;
  status = AudioUnitSetProperty(in->unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input, 1,
                                &enabled, sizeof(enabled));
  if (status != noErr) {
    fail_status(err, err_length, "that microphone cannot be turned on", status);
    goto failed;
  }
  status = AudioUnitSetProperty(in->unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0,
                                &device, sizeof(device));
  if (status != noErr) {
    fail_status(err, err_length, "that microphone is no longer connected", status);
    goto failed;
  }

  // The device side of the bus takes the device's own format, which is what
  // lets the unit resample at all; the client side then asks for float at that
  // channel count. Asking for Yapfast's channel count instead is refused, or
  // quietly answered with the device's, and the difference only shows up as
  // noise in someone's ear.
  AudioStreamBasicDescription device_side = heard_format(device);
  if (device_side.mSampleRate > 0 && device_side.mChannelsPerFrame > 0) {
    AudioUnitSetProperty(in->unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 1, &device_side,
                         sizeof(device_side));
  }
  uint32_t wanted_channels = device_side.mChannelsPerFrame > 0 ? device_side.mChannelsPerFrame : channels;
  AudioStreamBasicDescription format = client_format(rate, wanted_channels);
  status = AudioUnitSetProperty(in->unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, 1,
                                &format, sizeof(format));
  // The input side of the unit doesn't resample, so a microphone that won't
  // take Yapfast's rate is heard at its own, and yap_input_read converts.
  if (status != noErr && device_side.mSampleRate > 0) {
    format = client_format((uint32_t)device_side.mSampleRate, wanted_channels);
    status = AudioUnitSetProperty(in->unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, 1,
                                  &format, sizeof(format));
  }
  if (status != noErr) {
    fail_status(err, err_length, "that microphone will not take this format", status);
    goto failed;
  }

  AURenderCallbackStruct callback = {record_input, in};
  status = AudioUnitSetProperty(in->unit, kAudioOutputUnitProperty_SetInputCallback, kAudioUnitScope_Global,
                                1, &callback, sizeof(callback));
  if (status != noErr) {
    fail_status(err, err_length, "could not ask the unit for the microphone", status);
    goto failed;
  }
  status = AudioUnitInitialize(in->unit);
  if (status != noErr) {
    fail_status(err, err_length, "could not start that microphone", status);
    goto failed;
  }
  // The largest block the unit asks for, which is the buffer it renders into.
  UInt32 slice = 0;
  UInt32 size = sizeof(slice);
  if (AudioUnitGetProperty(in->unit, kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitScope_Global, 0,
                           &slice, &size) != noErr ||
      slice == 0) {
    slice = io_frames(device);
  }
  // What the unit says it renders is the truth from here.
  AudioStreamBasicDescription render = client_format(rate, wanted_channels);
  UInt32 render_size = sizeof(render);
  if (AudioUnitGetProperty(in->unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, 1, &render,
                           &render_size) == noErr &&
      render.mChannelsPerFrame > 0) {
    format = render;
  }
  in->render_channels = format.mChannelsPerFrame;
  in->render_rate = (uint32_t)format.mSampleRate;
  in->render_frame_bytes = 4 * in->render_channels;
  // The ring is measured in what the unit renders, and its alignment is one
  // frame of that, so this waits until the format is known. Opened against the
  // wrong frame size, a full ring stores part of a frame and the whole stream
  // after it arrives shifted by those bytes.
  size_t bytes = (size_t)ring_frames * in->render_frame_bytes;
  if (bytes == 0) {
    bytes = (size_t)(rate / 10) * in->render_frame_bytes;
  }
  if (ring_open(&in->ring, bytes + (size_t)io_frames(device) * in->render_frame_bytes * 4,
                in->render_frame_bytes) != YAPOK) {
    fail(err, err_length, "out of memory");
    goto failed;
  }
  in->recorded = calloc((size_t)slice * in->render_frame_bytes, 1);
  if (in->recorded == NULL) {
    fail(err, err_length, "out of memory");
    goto failed;
  }
  in->recorded_frames = slice;
  if (in->render_rate != rate && in->render_rate > 0) {
    // Rare, but a Bluetooth headset settles on 8 or 16 kHz and Yapfast wants
    // 48. The converter runs where Yapfast does, never in the unit's thread.
    AudioStreamBasicDescription from = client_format(in->render_rate, channels);
    AudioStreamBasicDescription to = client_format(rate, channels);
    if (AudioConverterNew(&from, &to, &in->converter) != noErr) {
      in->converter = NULL;
    }
  }
  status = AudioOutputUnitStart(in->unit);
  if (status != noErr) {
    fail_status(err, err_length, "could not record from that microphone", status);
    goto failed;
  }
  in->started = true;
  return in;

failed:
  // Whatever was set up before the failure goes too: a device that won't open
  // is tried again every half second.
  yap_input_free(in);
  return NULL;
}

/// Narrows the floats the ring holds to 16-bit samples, which is what
/// Discord's microphone and the input meter read.
static void narrow(const uint8_t *in, uint8_t *out, size_t floats) {
  const float *source = (const float *)in;
  int16_t *samples = (int16_t *)out;
  for (size_t at = 0; at < floats; at++) {
    float value = source[at] * 32768.0f;
    if (value > 32767.0f) {
      value = 32767.0f;
    } else if (value < -32768.0f) {
      value = -32768.0f;
    }
    samples[at] = (int16_t)value;
  }
}

/// Where the converter's source block is, and how much of it is left to give.
struct Source {
  const float *frames;
  size_t total;
  size_t left;
  uint32_t channels;
};

/// What `supply` says once the block is used up. Zero frames with `noErr`
/// would tell the converter the stream ended, and it would make nothing more
/// until reset; any other status only ends this call and keeps its state.
static const OSStatus USED_UP = 'used';

/// Hands the converter the block Yapfast just wove together, a little at a time.
static OSStatus supply(AudioConverterRef converter, UInt32 *frames, AudioBufferList *list,
                       AudioStreamPacketDescription **descriptions, void *context) {
  (void)converter;
  (void)descriptions;
  struct Source *source = context;
  if (source->left == 0) {
    *frames = 0;
    return USED_UP;
  }
  UInt32 given = *frames < source->left ? *frames : (UInt32)source->left;
  list->mNumberBuffers = 1;
  list->mBuffers[0].mData = (void *)(source->frames + (source->total - source->left) * source->channels);
  list->mBuffers[0].mDataByteSize = (UInt32)(given * sizeof(float) * source->channels);
  *frames = given;
  source->left -= given;
  return noErr;
}

/// Fills `bytes` with what the microphone hears next, waiting for it.
///
/// The ring holds the microphone's own layout, so this is where it becomes the
/// channels and rate Yapfast asked for: the channels are woven together, then
/// the rate is converted when the device settled on another. Both happen here,
/// on Yapfast's thread, since neither belongs in the unit's real-time one.
int yap_input_read(struct YAPInput *in, uint8_t *data, size_t bytes, char *err, size_t err_length) {
  size_t frames = bytes / ((in->float_samples ? 4 : 2) * in->channels);
  // A device at another rate needs proportionally more of it for the same time.
  size_t source_frames = in->converter != NULL && in->render_rate > 0
                             ? frames * in->render_rate / in->rate
                             : frames;
  if (source_frames == 0) {
    source_frames = 1;
  }
  size_t width = source_frames * in->render_frame_bytes;
  if (in->scratch_size < width) {
    free(in->scratch);
    in->scratch = malloc(width);
    in->scratch_size = in->scratch != NULL ? width : 0;
    if (in->scratch == NULL) {
      fail(err, err_length, "out of memory");
      return YAPERR;
    }
  }
  if (!ring_await(&in->ring, FILLED, width)) {
    fail(err, err_length, "that microphone stopped");
    return YAPERR;
  }
  ring_take(&in->ring, in->scratch, width);
  if (in->mapped_frames < source_frames) {
    free(in->mapped);
    in->mapped = calloc(source_frames, in->channels * sizeof(float));
    in->mapped_frames = in->mapped != NULL ? source_frames : 0;
    if (in->mapped == NULL) {
      fail(err, err_length, "out of memory");
      return YAPERR;
    }
  }
  // A mono microphone's sound goes to both of Yapfast's channels.
  for (size_t at = 0; at < source_frames; at++) {
    const float *from = (const float *)in->scratch + at * in->render_channels;
    for (uint32_t each = 0; each < in->channels; each++) {
      in->mapped[at * in->channels + each] = from[each < in->render_channels ? each : 0];
    }
  }
  const float *out = in->mapped;
  if (in->converter != NULL) {
    if (in->resampled_frames < frames) {
      free(in->resampled);
      in->resampled = calloc(frames, in->channels * sizeof(float));
      in->resampled_frames = in->resampled != NULL ? frames : 0;
      if (in->resampled == NULL) {
        fail(err, err_length, "out of memory");
        return YAPERR;
      }
    }
    struct Source source = {in->mapped, source_frames, source_frames, in->channels};
    AudioBufferList filled;
    filled.mNumberBuffers = 1;
    filled.mBuffers[0].mNumberChannels = in->channels;
    filled.mBuffers[0].mData = in->resampled;
    filled.mBuffers[0].mDataByteSize = (UInt32)(frames * 4 * in->channels);
    UInt32 made = (UInt32)frames;
    OSStatus status =
        AudioConverterFillComplexBuffer(in->converter, supply, &source, &made, &filled, NULL);
    if (status != noErr && status != USED_UP) {
      made = 0;
    }
    // Whatever the converter didn't reach is silence rather than the last
    // block played twice, which would hum.
    for (size_t at = (size_t)made * in->channels; at < frames * in->channels; at++) {
      in->resampled[at] = 0.0f;
    }
    out = in->resampled;
  }
  if (in->float_samples) {
    memcpy(data, out, bytes);
  } else {
    narrow((const uint8_t *)out, data, frames * in->channels);
  }
  return YAPOK;
}

void yap_input_free(struct YAPInput *in) {
  if (in == NULL) {
    return;
  }
  if (in->started) {
    AudioOutputUnitStop(in->unit);
  }
  ring_free(&in->ring);
  if (in->unit != NULL) {
    AudioUnitUninitialize(in->unit);
    AudioComponentInstanceDispose(in->unit);
  }
  free(in->scratch);
  free(in->recorded);
  free(in->mapped);
  free(in->resampled);
  if (in->converter != NULL) {
    AudioConverterDispose(in->converter);
  }
  free(in);
}
