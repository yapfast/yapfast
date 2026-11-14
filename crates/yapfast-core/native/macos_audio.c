/* CoreAudio (AudioToolbox) input and output streams for macOS (Intel & ARM). */
#include <AudioToolbox/AudioToolbox.h>
#include <CoreAudio/CoreAudio.h>
#include <CoreFoundation/CoreFoundation.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define NUM_BUFFERS 4

int yapfast_macos_raise_priority(int is_playback) {
    (void)is_playback;
    /* QOS_CLASS_USER_INTERACTIVE gives audio threads real-time-friendly scheduling
     * on both Apple Silicon and Intel Hackintosh/Ryzentosh without requiring root. */
    return pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
}

int yapfast_macos_default_device_uid(int input, char *buf, int cap) {
    if (!buf || cap <= 0) return -1;
    buf[0] = '\0';

    AudioObjectPropertyAddress addr = {
        input ? kAudioHardwarePropertyDefaultInputDevice : kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioDeviceID dev_id = kAudioObjectUnknown;
    UInt32 size = sizeof(dev_id);
    if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &addr, 0, NULL, &size, &dev_id) != noErr ||
        dev_id == kAudioObjectUnknown) {
        return -1;
    }

    AudioObjectPropertyAddress uid_addr = {
        kAudioDevicePropertyDeviceUID,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    CFStringRef uid = NULL;
    size = sizeof(uid);
    if (AudioObjectGetPropertyData(dev_id, &uid_addr, 0, NULL, &size, &uid) != noErr || !uid) {
        return -1;
    }
    Boolean ok = CFStringGetCString(uid, buf, cap, kCFStringEncodingUTF8);
    CFRelease(uid);
    return ok ? 0 : -1;
}

static void set_queue_device(AudioQueueRef queue, const char *device_uid) {
    if (!device_uid || device_uid[0] == '\0') return;
    CFStringRef cf_uid = CFStringCreateWithCString(kCFAllocatorDefault, device_uid, kCFStringEncodingUTF8);
    if (cf_uid) {
        AudioQueueSetProperty(queue, kAudioQueueProperty_CurrentDevice, &cf_uid, sizeof(cf_uid));
        CFRelease(cf_uid);
    }
}

static AudioStreamBasicDescription make_asbd(uint32_t rate, uint16_t channels, int is_float) {
    AudioStreamBasicDescription asbd;
    memset(&asbd, 0, sizeof(asbd));
    uint32_t bytes_per_sample = is_float ? 4 : 2;
    asbd.mSampleRate = (Float64)rate;
    asbd.mFormatID = kAudioFormatLinearPCM;
    asbd.mFormatFlags = kAudioFormatFlagIsPacked |
                        (is_float ? kAudioFormatFlagIsFloat : kAudioFormatFlagIsSignedInteger);
    asbd.mBytesPerPacket = bytes_per_sample * channels;
    asbd.mFramesPerPacket = 1;
    asbd.mBytesPerFrame = bytes_per_sample * channels;
    asbd.mChannelsPerFrame = channels;
    asbd.mBitsPerChannel = bytes_per_sample * 8;
    return asbd;
}

/* ---------------- Output ---------------- */

typedef struct {
    AudioQueueRef queue;
    AudioQueueBufferRef buffers[NUM_BUFFERS];
    AudioQueueBufferRef free_list[NUM_BUFFERS];
    int free_count;
    uint32_t rate;
    uint32_t frame_bytes;
    uint64_t queued_bytes;
    int stopped;
    pthread_mutex_t mu;
    pthread_cond_t cv;
} MacOutput;

static void output_cb(void *user, AudioQueueRef queue, AudioQueueBufferRef buf) {
    (void)queue;
    MacOutput *out = (MacOutput *)user;
    pthread_mutex_lock(&out->mu);
    if (out->queued_bytes >= buf->mAudioDataByteSize) {
        out->queued_bytes -= buf->mAudioDataByteSize;
    } else {
        out->queued_bytes = 0;
    }
    if (out->free_count < NUM_BUFFERS) {
        out->free_list[out->free_count++] = buf;
    }
    pthread_cond_broadcast(&out->cv);
    pthread_mutex_unlock(&out->mu);
}

int yapfast_macos_output_open(const char *device_uid, uint32_t rate, uint16_t channels,
                              int is_float, uint32_t buffer_bytes, void **out_handle) {
    MacOutput *out = (MacOutput *)calloc(1, sizeof(MacOutput));
    if (!out) return -1;
    pthread_mutex_init(&out->mu, NULL);
    pthread_cond_init(&out->cv, NULL);
    out->rate = rate ? rate : 48000;
    out->frame_bytes = (uint32_t)channels * (is_float ? 4u : 2u);
    if (buffer_bytes == 0) {
        buffer_bytes = (out->rate / 50) * out->frame_bytes; /* 20ms */
    }

    AudioStreamBasicDescription asbd = make_asbd(out->rate, channels, is_float);
    OSStatus err = AudioQueueNewOutput(&asbd, output_cb, out, NULL, NULL, 0, &out->queue);
    if (err != noErr) {
        pthread_mutex_destroy(&out->mu);
        pthread_cond_destroy(&out->cv);
        free(out);
        return (int)err;
    }

    set_queue_device(out->queue, device_uid);

    for (int i = 0; i < NUM_BUFFERS; i++) {
        err = AudioQueueAllocateBuffer(out->queue, buffer_bytes, &out->buffers[i]);
        if (err != noErr) {
            AudioQueueDispose(out->queue, true);
            pthread_mutex_destroy(&out->mu);
            pthread_cond_destroy(&out->cv);
            free(out);
            return (int)err;
        }
        out->free_list[out->free_count++] = out->buffers[i];
    }

    err = AudioQueueStart(out->queue, NULL);
    if (err != noErr) {
        AudioQueueDispose(out->queue, true);
        pthread_mutex_destroy(&out->mu);
        pthread_cond_destroy(&out->cv);
        free(out);
        return (int)err;
    }

    *out_handle = out;
    return 0;
}

int yapfast_macos_output_write(void *handle, const uint8_t *data, uint32_t len) {
    MacOutput *out = (MacOutput *)handle;
    uint32_t offset = 0;
    while (offset < len) {
        pthread_mutex_lock(&out->mu);
        while (out->free_count == 0 && !out->stopped) {
            pthread_cond_wait(&out->cv, &out->mu);
        }
        if (out->stopped) {
            pthread_mutex_unlock(&out->mu);
            return -1;
        }
        AudioQueueBufferRef buf = out->free_list[--out->free_count];
        uint32_t chunk = len - offset;
        if (chunk > buf->mAudioDataBytesCapacity) {
            chunk = buf->mAudioDataBytesCapacity;
        }
        memcpy(buf->mAudioData, data + offset, chunk);
        buf->mAudioDataByteSize = chunk;
        out->queued_bytes += chunk;
        pthread_mutex_unlock(&out->mu);

        OSStatus err = AudioQueueEnqueueBuffer(out->queue, buf, 0, NULL);
        if (err != noErr) {
            return (int)err;
        }
        offset += chunk;
    }
    return 0;
}

int yapfast_macos_output_drain(void *handle) {
    MacOutput *out = (MacOutput *)handle;
    pthread_mutex_lock(&out->mu);
    while (out->free_count < NUM_BUFFERS && !out->stopped) {
        pthread_cond_wait(&out->cv, &out->mu);
    }
    pthread_mutex_unlock(&out->mu);
    return 0;
}

void yapfast_macos_output_flush(void *handle) {
    MacOutput *out = (MacOutput *)handle;
    AudioQueueReset(out->queue);
}

uint64_t yapfast_macos_output_latency_us(void *handle) {
    MacOutput *out = (MacOutput *)handle;
    pthread_mutex_lock(&out->mu);
    uint64_t bytes = out->queued_bytes;
    pthread_mutex_unlock(&out->mu);
    if (out->frame_bytes == 0 || out->rate == 0) return 0;
    uint64_t frames = bytes / out->frame_bytes;
    return (frames * 1000000ULL) / out->rate;
}

void yapfast_macos_output_close(void *handle) {
    MacOutput *out = (MacOutput *)handle;
    if (!out) return;
    pthread_mutex_lock(&out->mu);
    out->stopped = 1;
    pthread_cond_broadcast(&out->cv);
    pthread_mutex_unlock(&out->mu);

    AudioQueueStop(out->queue, true);
    AudioQueueDispose(out->queue, true);
    pthread_mutex_destroy(&out->mu);
    pthread_cond_destroy(&out->cv);
    free(out);
}

/* ---------------- Input ---------------- */

typedef struct {
    AudioQueueRef queue;
    AudioQueueBufferRef buffers[NUM_BUFFERS];
    uint8_t *ring;
    uint32_t ring_cap;
    uint32_t ring_head;
    uint32_t ring_tail;
    uint32_t ring_len;
    int stopped;
    pthread_mutex_t mu;
    pthread_cond_t cv;
} MacInput;

static void input_cb(void *user, AudioQueueRef queue, AudioQueueBufferRef buf,
                     const AudioTimeStamp *start_time, UInt32 num_packets,
                     const AudioStreamPacketDescription *packet_descs) {
    (void)start_time;
    (void)num_packets;
    (void)packet_descs;
    MacInput *inp = (MacInput *)user;

    pthread_mutex_lock(&inp->mu);
    if (!inp->stopped && buf->mAudioDataByteSize > 0) {
        const uint8_t *src = (const uint8_t *)buf->mAudioData;
        uint32_t n = buf->mAudioDataByteSize;
        if (n > inp->ring_cap) {
            src += (n - inp->ring_cap);
            n = inp->ring_cap;
        }
        /* Drop oldest bytes if ring is full so we never block the CoreAudio callback. */
        if (inp->ring_len + n > inp->ring_cap) {
            uint32_t drop = (inp->ring_len + n) - inp->ring_cap;
            inp->ring_head = (inp->ring_head + drop) % inp->ring_cap;
            inp->ring_len -= drop;
        }
        uint32_t first = inp->ring_cap - inp->ring_tail;
        if (first > n) first = n;
        memcpy(inp->ring + inp->ring_tail, src, first);
        if (n > first) {
            memcpy(inp->ring, src + first, n - first);
        }
        inp->ring_tail = (inp->ring_tail + n) % inp->ring_cap;
        inp->ring_len += n;
        pthread_cond_broadcast(&inp->cv);
    }
    int stopped = inp->stopped;
    pthread_mutex_unlock(&inp->mu);

    if (!stopped) {
        AudioQueueEnqueueBuffer(queue, buf, 0, NULL);
    }
}

int yapfast_macos_input_open(const char *device_uid, uint32_t rate, uint16_t channels,
                             int is_float, uint32_t fragment_bytes, void **out_handle) {
    MacInput *inp = (MacInput *)calloc(1, sizeof(MacInput));
    if (!inp) return -1;
    pthread_mutex_init(&inp->mu, NULL);
    pthread_cond_init(&inp->cv, NULL);

    uint32_t frame_bytes = (uint32_t)channels * (is_float ? 4u : 2u);
    if (fragment_bytes == 0) {
        fragment_bytes = ((rate ? rate : 48000) / 50) * frame_bytes;
    }
    inp->ring_cap = fragment_bytes * 8;
    if (inp->ring_cap < 65536) inp->ring_cap = 65536;
    inp->ring = (uint8_t *)malloc(inp->ring_cap);
    if (!inp->ring) {
        pthread_mutex_destroy(&inp->mu);
        pthread_cond_destroy(&inp->cv);
        free(inp);
        return -1;
    }

    AudioStreamBasicDescription asbd = make_asbd(rate ? rate : 48000, channels, is_float);
    OSStatus err = AudioQueueNewInput(&asbd, input_cb, inp, NULL, NULL, 0, &inp->queue);
    if (err != noErr) {
        free(inp->ring);
        pthread_mutex_destroy(&inp->mu);
        pthread_cond_destroy(&inp->cv);
        free(inp);
        return (int)err;
    }

    set_queue_device(inp->queue, device_uid);

    for (int i = 0; i < NUM_BUFFERS; i++) {
        err = AudioQueueAllocateBuffer(inp->queue, fragment_bytes, &inp->buffers[i]);
        if (err == noErr) {
            err = AudioQueueEnqueueBuffer(inp->queue, inp->buffers[i], 0, NULL);
        }
        if (err != noErr) {
            AudioQueueDispose(inp->queue, true);
            free(inp->ring);
            pthread_mutex_destroy(&inp->mu);
            pthread_cond_destroy(&inp->cv);
            free(inp);
            return (int)err;
        }
    }

    err = AudioQueueStart(inp->queue, NULL);
    if (err != noErr) {
        AudioQueueDispose(inp->queue, true);
        free(inp->ring);
        pthread_mutex_destroy(&inp->mu);
        pthread_cond_destroy(&inp->cv);
        free(inp);
        return (int)err;
    }

    *out_handle = inp;
    return 0;
}

int yapfast_macos_input_read(void *handle, uint8_t *dst, uint32_t len) {
    MacInput *inp = (MacInput *)handle;
    pthread_mutex_lock(&inp->mu);
    while (inp->ring_len < len && !inp->stopped) {
        pthread_cond_wait(&inp->cv, &inp->mu);
    }
    if (inp->stopped) {
        pthread_mutex_unlock(&inp->mu);
        return -1;
    }
    uint32_t first = inp->ring_cap - inp->ring_head;
    if (first > len) first = len;
    memcpy(dst, inp->ring + inp->ring_head, first);
    if (len > first) {
        memcpy(dst + first, inp->ring, len - first);
    }
    inp->ring_head = (inp->ring_head + len) % inp->ring_cap;
    inp->ring_len -= len;
    pthread_mutex_unlock(&inp->mu);
    return 0;
}

void yapfast_macos_input_close(void *handle) {
    MacInput *inp = (MacInput *)handle;
    if (!inp) return;
    pthread_mutex_lock(&inp->mu);
    inp->stopped = 1;
    pthread_cond_broadcast(&inp->cv);
    pthread_mutex_unlock(&inp->mu);

    AudioQueueStop(inp->queue, true);
    AudioQueueDispose(inp->queue, true);
    free(inp->ring);
    pthread_mutex_destroy(&inp->mu);
    pthread_cond_destroy(&inp->cv);
    free(inp);
}
