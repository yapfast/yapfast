//! Yapfast plugins in C.
//!
//! The guest ABI is small on purpose: Yapfast calls five exports, the plugin
//! has two imports, and everything between them is JSON. This header is the
//! whole of it — a plugin includes it, links `yapfast.c`, `json.c` and
//! `freestanding.c` beside its own file, and gets a module with nothing else in
//! it.
//!
//! There is no C library here. A plugin runs in Extism with no filesystem, no
//! clock and no allocator of its own, so the PDK keeps one input buffer, one
//! scratch buffer and one output buffer as ordinary static arrays, and copies
//! into them. What matters is that the memory a plugin touches is memory its
//! instance keeps for the rest of its life: keep the three buffers small, and a
//! plugin stays well under its page limit.
//!
//! A plugin is plain C: five functions with fixed names, of which it writes only
//! the ones it needs. `yapfast.c` supplies the rest as weak defaults, so a
//! plugin that adds one command writes one function.
//!
//! Build with plain clang: the module needs no SDK, and no WASI either.
//!
//! ```sh
//! clang --target=wasm32-unknown-unknown -ffreestanding -nostdlib -O2 \
//!       -Wl,--no-entry -Wl,--export-memory \
//!       -o plugin.wasm shout.c yapfast.c json.c freestanding.c
//! ```

#ifndef YAPFAST_PDK_H
#define YAPFAST_PDK_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/// One view of bytes: a payload, a field of one, or a string a plugin was given.
///
/// The bytes are not zero-terminated, because a payload's are not: they are the
/// bytes between the frame header and the end of the frame, and a plugin that
/// wants a C string copies them into its own buffer with [yf_copy].
typedef struct {
    const char *data;
    size_t len;
} yf_str;

/// A string literal as a [yf_str], which counts it rather than calling
/// `strlen`.
#define YF_STR(literal) ((yf_str){ (literal), sizeof(literal) - 1 })

/// A view of nothing, which is what an absent field is.
#define YF_NONE ((yf_str){ NULL, 0 })

/// Builds JSON into a buffer the plugin owns.
///
/// Every write is checked against the buffer's end. One that doesn't fit sets
/// [failed] and is dropped, because half a JSON document is worse than an empty
/// answer: the caller sees the failure and answers the default instead.
typedef struct {
    char *data;
    size_t cap;
    size_t len;
    bool failed;
} yf_buf;

// ---------------------------------------------------------------------------
// Host calls
// ---------------------------------------------------------------------------

/// The answer to a host call: its JSON, copied by the caller into `out`.
///
/// Returns the answer's length, or -1 when it doesn't fit in `cap`. The answer
/// is one of `{"ok": ...}`, `"unsupported"`, `{"denied": {"reason": ...}}`,
/// `{"pending": {"tag": ...}}` or `{"error": {"message": ...}}`, and it is the
/// plugin's to read: a call that this build doesn't have answers `unsupported`
/// rather than failing to link, so a plugin can ask before it relies on one.
///
/// `call` is the whole `{"method": ..., "payload": ...}` object, which
/// [yf_method] builds for the common case.
ptrdiff_t yf_call(yf_str call, char *out, size_t cap);

/// One call with no answer: a log line, or something to publish and forget.
void yf_post(yf_str call);

/// Sends `payload` under `method` and returns the answer as [yf_call] does.
ptrdiff_t yf_method(yf_str method, yf_str payload, char *out, size_t cap);

/// Whether an answer is `{"ok": ...}`, and if so where its payload is.
///
/// Returns true and sets `out` to the payload's bytes when the method ran,
/// false for every other answer. A plugin that cares why it failed reads the
/// answer itself.
bool yf_answer_ok(yf_str answer, yf_str *out);

/// Writes one line into the plugin's own log, which its page shows.
///
/// Logging never fails a handler, and nothing tells the plugin whether the line
/// arrived: treat it as a hint, and storage as the place a value that matters
/// goes. `level` is `"error"`, `"warn"` or `"info"`.
void yf_log(yf_str level, yf_str message);

/// Reads one of the plugin's own keys, if it has one.
///
/// Returns the value's length, 0 when the key isn't there, or -1 when the value
/// is larger than `cap`. The value is JSON: storage holds a JSON value per key.
ptrdiff_t yf_storage_get(yf_str key, char *out, size_t cap);

/// Writes one key, replacing what was there. `value` is a JSON value.
///
/// Returns false when Yapfast refused: a missing capability, or the plugin's
/// storage quota reached.
bool yf_storage_set(yf_str key, yf_str value);

/// Deletes one key. Deleting a key that isn't there isn't a failure.
bool yf_storage_delete(yf_str key);

// ---------------------------------------------------------------------------
// JSON, reading it
// ---------------------------------------------------------------------------

/// Finds `key` in the object `json`, setting `value` to its raw bytes.
///
/// The bytes are JSON, so a string field keeps its quotes and its escapes: read
/// one with [yf_string]. Returns false when the object has no such key. Only
/// top-level keys are looked at; [yf_path] reaches further in.
bool yf_get(yf_str json, yf_str key, yf_str *value);

/// Finds a dotted path, such as `options.text`, inside `json`.
///
/// Each step reads a key of an object, so an array can't be stepped into: take
/// the array with [yf_get] first and read its items with [yf_items].
bool yf_path(yf_str json, yf_str path, yf_str *value);

/// Reads a JSON string into `out`, undoing its escapes.
///
/// Returns the number of bytes written, or -1 when it doesn't fit. A string
/// holding a `\u` escape becomes its UTF-8 bytes.
ptrdiff_t yf_string(yf_str value, char *out, size_t cap);

/// Whether a value is the JSON string `want`.
bool yf_string_is(yf_str value, yf_str want);

/// Reads a JSON number that is a whole count.
bool yf_uint(yf_str value, uint64_t *out);

/// Reads `true` or `false`.
bool yf_bool(yf_str value, bool *out);

/// Splits the array `json` into its items, at most `max`.
///
/// Returns how many items it found, which may be more than `max`: a plugin that
/// wants them all calls again with a larger array, and one that wants the first
/// few says so by the size it passed.
size_t yf_items(yf_str json, yf_str *items, size_t max);

/// Walks an array's items one at a time, without an array to hold them.
///
/// A batch can carry more events than a plugin wants to size an array for, so
/// this is how most plugins read one: loop while [yf_iter_next] says there's
/// another, and never hold more than the one in hand.
typedef struct {
    yf_str json;
    size_t at;
    bool started;
} yf_iter;

void yf_iter_init(yf_iter *it, yf_str array);

/// Sets `item` to the next item's raw bytes. False when the array is finished.
bool yf_iter_next(yf_iter *it, yf_str *item);

// ---------------------------------------------------------------------------
// JSON, writing it
// ---------------------------------------------------------------------------

/// Points a writer at memory the plugin owns.
void yf_buf_init(yf_buf *buf, char *data, size_t cap);

/// Appends bytes that are already JSON.
void yf_raw(yf_buf *buf, yf_str json);

/// Appends a JSON string, quoting and escaping `text`.
void yf_quoted(yf_buf *buf, yf_str text);

/// Appends a JSON count.
void yf_number(yf_buf *buf, uint64_t number);

/// Appends `true` or `false`.
void yf_boolean(yf_buf *buf, bool value);

// ---------------------------------------------------------------------------
// Answers
// ---------------------------------------------------------------------------

/// `"none"`: the command did nothing else worth showing.
void yf_none(yf_buf *buf);

/// A line for the person to see in the room they typed the command in.
void yf_notice(yf_buf *buf, yf_str text);

/// The command failed, and this is why.
void yf_failed(yf_buf *buf, yf_str message);

/// The command's own JSON, for a future caller of its own to read.
void yf_value(yf_buf *buf, yf_str json);

/// `"unchanged"`: the message goes as it was.
void yf_unchanged(yf_buf *buf);

/// `"block"`: the message must not go, and this is why the person is told.
void yf_block(yf_buf *buf, yf_str reason);

/// A changed draft, which the next plugin in the chain sees.
///
/// The whole subject is replaced, so every field is here: a plugin that changes
/// one copies the rest from what it was given. `attachments` is the JSON array
/// from the subject, and `reply_to` is a JSON string or [YF_NONE].
void yf_changed_draft(yf_buf *buf,
                      yf_str room,
                      yf_str body,
                      yf_str reply_to,
                      yf_str attachments);

// ---------------------------------------------------------------------------
// What a plugin writes
// ---------------------------------------------------------------------------
//
// A plugin implements the handlers it needs. The rest are no-ops from
// `src/yapfast.c`, so a plugin that adds a command writes one function and no
// boilerplate.

/// The instance is new, or was evicted and came back.
///
/// `payload` is `{"api": 1, "plugin": "com.example.shout", "state": ...}`, where
/// `state` is what the last [yapfast_deactivate] handed on, if anything. It runs
/// before anything else, so it is where a plugin reads its storage.
void yapfast_activate(yf_str payload);

/// The instance is going away. Return what the next activation should be given,
/// as a JSON string, or [YF_NONE].
///
/// This is a convenience and not a lifeline: an instance that trapped or ran
/// past its deadline never gets here, and an eviction may not either. Storage is
/// what survives.
yf_str yapfast_deactivate(void);

/// One event. `event` is the envelope, so `kind` says what it is.
void yapfast_event(yf_str event);

/// The answer to a call the plugin left pending arrived.
///
/// `answer` is the `HostReturn` from the ABI. Nothing is returned: the plugin
/// has already gone on with its work.
void yapfast_result(uint64_t call, yf_str answer);

/// A hook, before something goes out. Write the outcome into `answer`.
///
/// The default is [yf_unchanged], and a hook fails open: whatever the plugin
/// does, the message sends.
void yapfast_hook(yf_str request, yf_buf *answer);

/// A command a person typed. Write the outcome into `answer`.
///
/// The default is [yf_none]. The options were checked against the manifest
/// before the plugin was called, so a required one is there.
void yapfast_command(yf_str invocation, yf_buf *answer);

#endif
