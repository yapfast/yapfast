//! The five exports, and the buffers behind them.
//!
//! A plugin includes `yapfast.h` and writes the handlers it needs; this file
//! supplies the rest as weak defaults and does the marshalling in between. It is
//! compiled into every C plugin, so it stays small: three static buffers, no
//! allocator, and no C library at all (a freestanding `wasm32` build has none to
//! link).
//!
//! The buffers, and why there are three: a payload is being read while a host
//! call's answer arrives, so the answer can't go into the payload's place. The
//! output is built last and handed to Extism, which copies it into memory of its
//! own. A bigger payload than [YF_INPUT_MAX] is refused rather than cut, because
//! half a JSON document is worse than a fault a person can read.

#include "yapfast.h"
#include "imports.h"

#ifndef YF_INPUT_MAX
#define YF_INPUT_MAX (96 * 1024)
#endif

#ifndef YF_SCRATCH_MAX
#define YF_SCRATCH_MAX (32 * 1024)
#endif

#ifndef YF_OUTPUT_MAX
#define YF_OUTPUT_MAX (96 * 1024)
#endif

static char input_buffer[YF_INPUT_MAX];
static char scratch_buffer[YF_SCRATCH_MAX];
static char output_buffer[YF_OUTPUT_MAX];

/// Copies bytes into memory Extism keeps for this instance.
///
/// A handle is opaque, so this goes through the kernel's own store: eight bytes
/// at a time where it can, and a byte at a time for the tail. Casting a handle
/// to a pointer would compile and quietly write somewhere nobody reads.
static void write_bytes(uint64_t handle, const char *bytes, size_t len) {
    size_t at = 0;
    while (at + 8 <= len) {
        uint64_t word = 0;
        for (size_t i = 0; i < 8; i++) {
            word |= (uint64_t)(unsigned char)bytes[at + i] << (i * 8);
        }
        yf_import_store_u64(handle + at, word);
        at += 8;
    }
    while (at < len) {
        yf_import_store_u8(handle + at, (uint8_t)bytes[at]);
        at++;
    }
}

/// Copies bytes out of memory Extism keeps for this instance.
static void read_bytes(uint64_t handle, char *out, size_t len) {
    size_t at = 0;
    while (at + 8 <= len) {
        uint64_t word = yf_import_load_u64(handle + at);
        for (size_t i = 0; i < 8; i++) {
            out[at + i] = (char)((word >> (i * 8)) & 0xFF);
        }
        at += 8;
    }
    while (at < len) {
        out[at] = (char)yf_import_load_u8(handle + at);
        at++;
    }
}

/// Allocates `len` bytes of Extism memory and copies `bytes` into it.
static uint64_t copy_in(const char *bytes, size_t len) {
    uint64_t handle = yf_import_alloc(len);
    if (handle == 0) return 0;
    write_bytes(handle, bytes, len);
    return handle;
}

/// Tells Yapfast a call failed, in words a person can read.
static void refuse(yf_str reason) {
    // The message lives in Extism's memory, which frees what it is given once it
    // has copied the bytes out.
    uint64_t handle = copy_in(reason.data, reason.len);
    if (handle == 0) return;
    yf_import_error_set(handle);
}

/// Reads this call's payload into the input buffer.
static yf_str take_input(void) {
    uint64_t length = yf_import_input_length();
    if (length > YF_INPUT_MAX) {
        refuse(YF_STR("the payload is larger than this PDK's input buffer"));
        return YF_NONE;
    }
    // Eight bytes at a time where it can: a byte at a time is a host call each.
    uint64_t at_byte = 0;
    while (at_byte + 8 <= length) {
        uint64_t word = yf_import_input_load_u64(at_byte);
        for (size_t i = 0; i < 8; i++) {
            input_buffer[at_byte + i] = (char)((word >> (i * 8)) & 0xFF);
        }
        at_byte += 8;
    }
    while (at_byte < length) {
        input_buffer[at_byte] = (char)yf_import_input_load_u8(at_byte);
        at_byte++;
    }
    return (yf_str){ input_buffer, (size_t)length };
}

/// Hands `answer` to Extism as this call's output.
static void give_output(yf_str answer) {
    uint64_t handle = copy_in(answer.data, answer.len);
    if (handle == 0) {
        refuse(YF_STR("this instance has no memory left for an answer"));
        return;
    }
    yf_import_output_set(handle, answer.len);
}

/// Copies a returned handle's bytes into `out` and frees it.
static ptrdiff_t take_handle(uint64_t handle, char *out, size_t cap) {
    if (handle == 0) return -1;
    uint64_t length = yf_import_length(handle);
    ptrdiff_t wrote = -1;
    if (length <= cap) {
        read_bytes(handle, out, (size_t)length);
        wrote = (ptrdiff_t)length;
    }
    yf_import_free(handle);
    return wrote;
}

ptrdiff_t yf_call(yf_str call, char *out, size_t cap) {
    if (call.data == NULL || call.len > YF_SCRATCH_MAX) return -1;
    uint64_t handle = copy_in(call.data, call.len);
    if (handle == 0) return -1;
    uint64_t answer = yf_import_call(handle);
    // The request is this module's to free once the helper has read it.
    yf_import_free(handle);
    return take_handle(answer, out, cap);
}

void yf_post(yf_str call) {
    if (call.data == NULL || call.len > YF_SCRATCH_MAX) return;
    uint64_t handle = copy_in(call.data, call.len);
    if (handle == 0) return;
    yf_import_post(handle);
    yf_import_free(handle);
}

ptrdiff_t yf_method(yf_str method, yf_str payload, char *out, size_t cap) {
    char call[YF_SCRATCH_MAX];
    yf_buf buf;
    yf_buf_init(&buf, call, sizeof(call));
    yf_raw(&buf, YF_STR("{\"method\":"));
    yf_quoted(&buf, method);
    yf_raw(&buf, YF_STR(",\"payload\":"));
    yf_raw(&buf, payload.data != NULL ? payload : YF_STR("null"));
    yf_raw(&buf, YF_STR("}"));
    if (buf.failed) return -1;
    return yf_call((yf_str){ call, buf.len }, out, cap);
}

bool yf_answer_ok(yf_str answer, yf_str *out) {
    return yf_get(answer, YF_STR("ok"), out);
}

void yf_log(yf_str level, yf_str message) {
    char payload[1024];
    yf_buf buf;
    yf_buf_init(&buf, payload, sizeof(payload));
    yf_raw(&buf, YF_STR("{\"level\":"));
    yf_quoted(&buf, level);
    yf_raw(&buf, YF_STR(",\"message\":"));
    yf_quoted(&buf, message);
    yf_raw(&buf, YF_STR("}"));
    if (buf.failed) return;
    char call[1200];
    yf_buf one;
    yf_buf_init(&one, call, sizeof(call));
    yf_raw(&one, YF_STR("{\"method\":\"log\",\"payload\":"));
    yf_raw(&one, (yf_str){ payload, buf.len });
    yf_raw(&one, YF_STR("}"));
    if (one.failed) return;
    yf_post((yf_str){ call, one.len });
}

/// One answer, read straight after the call that brought it: nothing outlives
/// the handler it belongs to, so one buffer is enough for every call.
static bool method_call(yf_str method, yf_str payload, yf_str *answer) {
    ptrdiff_t length = yf_method(method, payload, scratch_buffer, sizeof(scratch_buffer));
    if (length < 0) return false;
    answer->data = scratch_buffer;
    answer->len = (size_t)length;
    return true;
}

/// A storage call: the method, its key, and a value for the one that sets one.
static bool storage_call(yf_str method, yf_str key, yf_str value, yf_str *answer) {
    char payload[1024];
    yf_buf buf;
    yf_buf_init(&buf, payload, sizeof(payload));
    yf_raw(&buf, YF_STR("{\"key\":"));
    yf_quoted(&buf, key);
    if (value.data != NULL) {
        yf_raw(&buf, YF_STR(",\"value\":"));
        yf_raw(&buf, value);
    }
    yf_raw(&buf, YF_STR("}"));
    if (buf.failed) return false;
    return method_call(method, (yf_str){ payload, buf.len }, answer);
}

ptrdiff_t yf_storage_get(yf_str key, char *out, size_t cap) {
    yf_str answer;
    if (!storage_call(YF_STR("storage.get"), key, YF_NONE, &answer)) return -1;
    yf_str value;
    if (!yf_answer_ok(answer, &value)) return 0;
    return yf_string(value, out, cap);
}

bool yf_storage_set(yf_str key, yf_str value) {
    yf_str answer;
    yf_str read;
    if (!storage_call(YF_STR("storage.set"), key, value, &answer)) return false;
    return yf_answer_ok(answer, &read);
}

bool yf_storage_delete(yf_str key) {
    yf_str answer;
    yf_str read;
    if (!storage_call(YF_STR("storage.delete"), key, YF_NONE, &answer)) return false;
    return yf_answer_ok(answer, &read);
}

// ---------------------------------------------------------------------------
// The exports Yapfast calls
// ---------------------------------------------------------------------------

// A plugin that doesn't implement one of these gets the default: do nothing, and
// answer with what the ABI says an untouched hook or command answers. They are
// weak so that a plugin's own definition is the one that is linked.
__attribute__((weak)) void yapfast_activate(yf_str payload) {
    (void)payload;
}

__attribute__((weak)) yf_str yapfast_deactivate(void) {
    return YF_NONE;
}

__attribute__((weak)) void yapfast_event(yf_str event) {
    (void)event;
}

__attribute__((weak)) void yapfast_result(uint64_t call, yf_str answer) {
    (void)call;
    (void)answer;
}

__attribute__((weak)) void yapfast_hook(yf_str request, yf_buf *answer) {
    (void)request;
    yf_unchanged(answer);
}

__attribute__((weak)) void yapfast_command(yf_str invocation, yf_buf *answer) {
    (void)invocation;
    yf_none(answer);
}

__attribute__((export_name("yf_activate"))) void yf_export_activate(void) {
    yapfast_activate(take_input());
}

__attribute__((export_name("yf_deactivate"))) void yf_export_deactivate(void) {
    yf_str state = yapfast_deactivate();
    // Nothing to hand on is `null`, which is what the ABI's optional answer is.
    if (state.data == NULL) {
        give_output(YF_STR("null"));
        return;
    }
    yf_buf buf;
    yf_buf_init(&buf, output_buffer, sizeof(output_buffer));
    yf_raw(&buf, YF_STR("{\"state\":"));
    yf_raw(&buf, state);
    yf_raw(&buf, YF_STR("}"));
    if (buf.failed) {
        give_output(YF_STR("null"));
        return;
    }
    give_output((yf_str){ output_buffer, buf.len });
}

__attribute__((export_name("yf_event"))) void yf_export_event(void) {
    yf_str batch = take_input();
    if (batch.data == NULL) return;
    // Answers first: a plugin that asked something reads its answer before the
    // event that asked is looked at again, which is the order the ABI describes.
    yf_str results;
    if (yf_get(batch, YF_STR("results"), &results)) {
        yf_iter it;
        yf_iter_init(&it, results);
        yf_str result;
        while (yf_iter_next(&it, &result)) {
            uint64_t call = 0;
            yf_str number;
            yf_str answer = YF_NONE;
            if (yf_get(result, YF_STR("call"), &number)) yf_uint(number, &call);
            yf_get(result, YF_STR("answer"), &answer);
            yapfast_result(call, answer);
        }
    }
    yf_str events;
    if (yf_get(batch, YF_STR("events"), &events)) {
        yf_iter it;
        yf_iter_init(&it, events);
        yf_str event;
        while (yf_iter_next(&it, &event)) yapfast_event(event);
    }
}

__attribute__((export_name("yf_hook"))) void yf_export_hook(void) {
    yf_str request = take_input();
    yf_buf buf;
    yf_buf_init(&buf, output_buffer, sizeof(output_buffer));
    yapfast_hook(request, &buf);
    // A hook fails open, so an answer that couldn't be built says "unchanged".
    if (buf.failed) {
        yf_buf_init(&buf, output_buffer, sizeof(output_buffer));
        yf_unchanged(&buf);
    }
    give_output((yf_str){ output_buffer, buf.len });
}

__attribute__((export_name("yf_command"))) void yf_export_command(void) {
    yf_str invocation = take_input();
    yf_buf buf;
    yf_buf_init(&buf, output_buffer, sizeof(output_buffer));
    yapfast_command(invocation, &buf);
    if (buf.failed) {
        yf_buf_init(&buf, output_buffer, sizeof(output_buffer));
        yf_failed(&buf, YF_STR("the plugin's answer was too large"));
    }
    give_output((yf_str){ output_buffer, buf.len });
}
