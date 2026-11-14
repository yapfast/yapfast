/* CoreAudio (AudioToolbox) input and output streams for macOS (Intel & ARM). */
#include <AudioToolbox/AudioToolbox.h>
#include <CoreAudio/CoreAudio.h>
#include <CoreFoundation/CoreFoundation.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#define NUM_BUFFERS 4

int yapfast_macos_raise_priority(int is_playback) {
    (void)is_playback;
    /* QOS_CLASS_USER_INTERACTIVE gives audio threads real-time-friendly scheduling
     * on both Apple Silicon and Intel Hackintosh/Ryzentosh without requiring root. */
    return pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
}

static void trim_ascii(char *s) {
    if (!s) return;
    size_t len = strlen(s);
    while (len > 0 && (s[len - 1] == ' ' || s[len - 1] == '\t' || s[len - 1] == '\r' || s[len - 1] == '\n')) {
        s[--len] = '\0';
    }
    size_t start = 0;
    while (s[start] == ' ' || s[start] == '\t' || s[start] == '\r' || s[start] == '\n') {
        start++;
    }
    if (start > 0) {
        memmove(s, s + start, len - start + 1);
    }
}

static int device_has_channels(AudioDeviceID dev_id, int is_input) {
    if (dev_id == kAudioObjectUnknown) return 0;
    AudioObjectPropertyAddress addr = {
        kAudioDevicePropertyStreamConfiguration,
        is_input ? kAudioDevicePropertyScopeInput : kAudioDevicePropertyScopeOutput,
        kAudioObjectPropertyElementMain
    };
    UInt32 size = 0;
    if (AudioObjectGetPropertyDataSize(dev_id, &addr, 0, NULL, &size) != noErr || size == 0) {
        return 0;
    }
    AudioBufferList *list = (AudioBufferList *)malloc(size);
    if (!list) return 0;
    UInt32 total = 0;
    if (AudioObjectGetPropertyData(dev_id, &addr, 0, NULL, &size, list) == noErr) {
        for (UInt32 i = 0; i < list->mNumberBuffers; i++) {
            total += list->mBuffers[i].mNumberChannels;
        }
    }
    free(list);
    return total > 0;
}

static int get_device_string_prop(AudioDeviceID dev_id, AudioObjectPropertySelector sel, char *buf, int cap) {
    if (!buf || cap <= 0 || dev_id == kAudioObjectUnknown) return -1;
    buf[0] = '\0';
    AudioObjectPropertyAddress addr = {
        sel,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    CFStringRef str = NULL;
    UInt32 size = sizeof(str);
    if (AudioObjectGetPropertyData(dev_id, &addr, 0, NULL, &size, &str) != noErr || !str) {
        return -1;
    }
    Boolean ok = CFStringGetCString(str, buf, cap, kCFStringEncodingUTF8);
    CFRelease(str);
    if (!ok) {
        buf[0] = '\0';
        return -1;
    }
    return 0;
}

static UInt32 get_device_transport(AudioDeviceID dev_id) {
    if (dev_id == kAudioObjectUnknown) return 0;
    AudioObjectPropertyAddress addr = {
        kAudioDevicePropertyTransportType,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    UInt32 transport = 0;
    UInt32 size = sizeof(transport);
    if (AudioObjectGetPropertyData(dev_id, &addr, 0, NULL, &size, &transport) != noErr) {
        return 0;
    }
    return transport;
}

static int is_bluetooth_transport(UInt32 transport) {
    return transport == kAudioDeviceTransportTypeBluetooth ||
           transport == kAudioDeviceTransportTypeBluetoothLE;
}

static int non_bt_input_priority(UInt32 transport) {
    if (is_bluetooth_transport(transport)) return 0;
    switch (transport) {
        case kAudioDeviceTransportTypeUSB:
        case kAudioDeviceTransportTypePCI:
        case kAudioDeviceTransportTypeThunderbolt:
        case kAudioDeviceTransportTypeFireWire:
        case kAudioDeviceTransportTypeDisplayPort:
        case kAudioDeviceTransportTypeHDMI:
            return 3; /* Prefer external wired microphone / interface when plugged in */
        case kAudioDeviceTransportTypeBuiltIn:
            return 2; /* Next prefer built-in microphone */
        case kAudioDeviceTransportTypeVirtual:
        case kAudioDeviceTransportTypeAggregate:
            return 0;
        default:
            return 1;
    }
}

int yapfast_macos_resolve_device_uid(const char *requested, int input, char *buf, int cap) {
    if (!buf || cap <= 0) return -1;
    buf[0] = '\0';

    char query[512];
    query[0] = '\0';
    if (requested) {
        if (strncmp(requested, "name:", 5) == 0) {
            requested += 5;
        }
        snprintf(query, sizeof(query), "%s", requested);
        trim_ascii(query);
        if (strcasecmp(query, "default") == 0 ||
            strncasecmp(query, "default:", 8) == 0 ||
            strncasecmp(query, "default -", 9) == 0) {
            query[0] = '\0';
        }
    }

    AudioObjectPropertyAddress list_addr = {
        kAudioHardwarePropertyDevices,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    UInt32 list_size = 0;
    AudioDeviceID *devs = NULL;
    UInt32 dev_count = 0;
    if (AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &list_addr, 0, NULL, &list_size) == noErr &&
        list_size >= sizeof(AudioDeviceID)) {
        devs = (AudioDeviceID *)malloc(list_size);
        if (devs && AudioObjectGetPropertyData(kAudioObjectSystemObject, &list_addr, 0, NULL, &list_size, devs) == noErr) {
            dev_count = list_size / sizeof(AudioDeviceID);
        }
    }

    /* If a specific device name or UID was requested, find its CoreAudio DeviceUID in the matching scope. */
    if (query[0] != '\0') {
        int found = -1;
        for (UInt32 i = 0; i < dev_count; i++) {
            if (!device_has_channels(devs[i], input)) continue;
            char uid[512], name[512], uid_trimmed[512];
            if (get_device_string_prop(devs[i], kAudioDevicePropertyDeviceUID, uid, sizeof(uid)) != 0) {
                continue;
            }
            snprintf(uid_trimmed, sizeof(uid_trimmed), "%s", uid);
            trim_ascii(uid_trimmed);
            if (strcmp(uid, query) == 0 || strcmp(uid_trimmed, query) == 0) {
                snprintf(buf, (size_t)cap, "%s", uid);
                found = 0;
                break;
            }
            if (get_device_string_prop(devs[i], kAudioObjectPropertyName, name, sizeof(name)) == 0) {
                trim_ascii(name);
                if (strcmp(name, query) == 0) {
                    snprintf(buf, (size_t)cap, "%s", uid);
                    found = 0;
                    break;
                }
            }
        }
        free(devs);
        return found;
    }

    /* Default device requested. */
    AudioObjectPropertyAddress def_addr = {
        input ? kAudioHardwarePropertyDefaultInputDevice : kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioDeviceID def_id = kAudioObjectUnknown;
    UInt32 size = sizeof(def_id);
    if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &def_addr, 0, NULL, &size, &def_id) != noErr ||
        def_id == kAudioObjectUnknown) {
        free(devs);
        return -1;
    }

    /* If the system default input is a Bluetooth headset, opening its mic forces HFP/SCO hands-free
     * mode (degrading headphone output to mono 8/16 kHz and disrupting 2.4 GHz Wi-Fi). When another
     * physical microphone (wired USB/jack/PCI or built-in) is available, prefer it for Default input. */
    if (input && is_bluetooth_transport(get_device_transport(def_id))) {
        AudioDeviceID best_id = kAudioObjectUnknown;
        int best_prio = 0;
        for (UInt32 i = 0; i < dev_count; i++) {
            if (!device_has_channels(devs[i], 1)) continue;
            int prio = non_bt_input_priority(get_device_transport(devs[i]));
            if (prio > best_prio) {
                best_prio = prio;
                best_id = devs[i];
            }
        }
        if (best_id != kAudioObjectUnknown) {
            def_id = best_id;
        }
    }

    free(devs);
    return get_device_string_prop(def_id, kAudioDevicePropertyDeviceUID, buf, cap);
}

int yapfast_macos_default_device_uid(int input, char *buf, int cap) {
    return yapfast_macos_resolve_device_uid(NULL, input, buf, cap);
}

static int set_queue_device(AudioQueueRef queue, const char *device_uid, int is_input) {
    char resolved[512];
    if (device_uid && device_uid[0] != '\0') {
        if (yapfast_macos_resolve_device_uid(device_uid, is_input, resolved, sizeof(resolved)) != 0) {
            if (yapfast_macos_resolve_device_uid(NULL, is_input, resolved, sizeof(resolved)) != 0) {
                return 0;
            }
        }
    } else if (is_input) {
        if (yapfast_macos_resolve_device_uid(NULL, 1, resolved, sizeof(resolved)) != 0) {
            return 0;
        }
    } else {
        return 0;
    }
    CFStringRef cf_uid = CFStringCreateWithCString(kCFAllocatorDefault, resolved, kCFStringEncodingUTF8);
    if (!cf_uid) return -1;
    OSStatus err = AudioQueueSetProperty(queue, kAudioQueueProperty_CurrentDevice, &cf_uid, sizeof(cf_uid));
    CFRelease(cf_uid);
    return (err == noErr) ? 0 : (int)err;
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
    uint32_t rate;
    uint32_t frame_bytes;
    uint32_t buffer_bytes;
    uint8_t *ring;
    uint32_t ring_cap;
    uint32_t ring_head;
    uint32_t ring_tail;
    uint32_t ring_len;
    int stopped;
    int failed;
    pthread_mutex_t mu;
    pthread_cond_t cv;
} MacOutput;

static void output_cb(void *user, AudioQueueRef queue, AudioQueueBufferRef buf) {
    MacOutput *out = (MacOutput *)user;
    pthread_mutex_lock(&out->mu);
    if (out->stopped || out->failed) {
        pthread_mutex_unlock(&out->mu);
        return;
    }

    uint32_t cap = out->buffer_bytes;
    if (cap > buf->mAudioDataBytesCapacity) {
        cap = buf->mAudioDataBytesCapacity;
    }
    uint32_t take = out->ring_len < cap ? out->ring_len : cap;
    if (take > 0) {
        uint32_t first = out->ring_cap - out->ring_head;
        if (first > take) first = take;
        memcpy(buf->mAudioData, out->ring + out->ring_head, first);
        if (take > first) {
            memcpy((uint8_t *)buf->mAudioData + first, out->ring, take - first);
        }
        out->ring_head = (out->ring_head + take) % out->ring_cap;
        out->ring_len -= take;
    }
    if (take < cap) {
        memset((uint8_t *)buf->mAudioData + take, 0, cap - take);
    }
    buf->mAudioDataByteSize = cap;
    pthread_cond_broadcast(&out->cv);
    pthread_mutex_unlock(&out->mu);

    OSStatus err = AudioQueueEnqueueBuffer(queue, buf, 0, NULL);
    if (err != noErr) {
        pthread_mutex_lock(&out->mu);
        out->failed = 1;
        pthread_cond_broadcast(&out->cv);
        pthread_mutex_unlock(&out->mu);
    }
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
    out->buffer_bytes = buffer_bytes;
    out->ring_cap = buffer_bytes * 10; /* ~200ms ring buffer */
    if (out->ring_cap < 65536) out->ring_cap = 65536;
    out->ring = (uint8_t *)malloc(out->ring_cap);
    if (!out->ring) {
        pthread_mutex_destroy(&out->mu);
        pthread_cond_destroy(&out->cv);
        free(out);
        return -1;
    }

    AudioStreamBasicDescription asbd = make_asbd(out->rate, channels, is_float);
    OSStatus err = AudioQueueNewOutput(&asbd, output_cb, out, NULL, NULL, 0, &out->queue);
    if (err != noErr) {
        free(out->ring);
        pthread_mutex_destroy(&out->mu);
        pthread_cond_destroy(&out->cv);
        free(out);
        return (int)err;
    }

    if (device_uid && device_uid[0]) {
        err = set_queue_device(out->queue, device_uid, 0);
        if (err != noErr) {
            AudioQueueDispose(out->queue, true);
            free(out->ring);
            pthread_mutex_destroy(&out->mu);
            pthread_cond_destroy(&out->cv);
            free(out);
            return (int)err;
        }
    }

    for (int i = 0; i < NUM_BUFFERS; i++) {
        err = AudioQueueAllocateBuffer(out->queue, buffer_bytes, &out->buffers[i]);
        if (err == noErr) {
            memset(out->buffers[i]->mAudioData, 0, buffer_bytes);
            out->buffers[i]->mAudioDataByteSize = buffer_bytes;
            err = AudioQueueEnqueueBuffer(out->queue, out->buffers[i], 0, NULL);
        }
        if (err != noErr) {
            AudioQueueDispose(out->queue, true);
            free(out->ring);
            pthread_mutex_destroy(&out->mu);
            pthread_cond_destroy(&out->cv);
            free(out);
            return (int)err;
        }
    }

    err = AudioQueueStart(out->queue, NULL);
    if (err != noErr) {
        AudioQueueDispose(out->queue, true);
        free(out->ring);
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
    pthread_mutex_lock(&out->mu);
    while (offset < len) {
        while (out->ring_len == out->ring_cap && !out->stopped && !out->failed) {
            pthread_cond_wait(&out->cv, &out->mu);
        }
        if (out->stopped || out->failed) {
            pthread_mutex_unlock(&out->mu);
            return -1;
        }
        uint32_t space = out->ring_cap - out->ring_len;
        uint32_t chunk = len - offset;
        if (chunk > space) chunk = space;
        uint32_t first = out->ring_cap - out->ring_tail;
        if (first > chunk) first = chunk;
        memcpy(out->ring + out->ring_tail, data + offset, first);
        if (chunk > first) {
            memcpy(out->ring, data + offset + first, chunk - first);
        }
        out->ring_tail = (out->ring_tail + chunk) % out->ring_cap;
        out->ring_len += chunk;
        offset += chunk;
    }
    pthread_mutex_unlock(&out->mu);
    return 0;
}

int yapfast_macos_output_drain(void *handle) {
    MacOutput *out = (MacOutput *)handle;
    pthread_mutex_lock(&out->mu);
    while (out->ring_len > 0 && !out->stopped && !out->failed) {
        pthread_cond_wait(&out->cv, &out->mu);
    }
    int failed = out->failed;
    pthread_mutex_unlock(&out->mu);
    return failed ? -1 : 0;
}

void yapfast_macos_output_flush(void *handle) {
    MacOutput *out = (MacOutput *)handle;
    pthread_mutex_lock(&out->mu);
    out->ring_head = 0;
    out->ring_tail = 0;
    out->ring_len = 0;
    pthread_cond_broadcast(&out->cv);
    pthread_mutex_unlock(&out->mu);
}

uint64_t yapfast_macos_output_latency_us(void *handle) {
    MacOutput *out = (MacOutput *)handle;
    pthread_mutex_lock(&out->mu);
    uint64_t bytes = out->ring_len;
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
    free(out->ring);
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

    if (device_uid && device_uid[0]) {
        err = set_queue_device(inp->queue, device_uid, 1);
        if (err != noErr) {
            AudioQueueDispose(inp->queue, true);
            free(inp->ring);
            pthread_mutex_destroy(&inp->mu);
            pthread_cond_destroy(&inp->cv);
            free(inp);
            return (int)err;
        }
    }

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
