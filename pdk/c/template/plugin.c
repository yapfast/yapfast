//! This plugin, in C.
//!
//! The five handlers below are all of them; delete the ones this plugin doesn't
//! need and the PDK's defaults take over. `build.sh` links this file with the
//! PDK's three and writes `plugin.wasm` beside `yapfast-plugin.json`.

#include "../yapfast.h"

// Yapfast gave this instance a new life: read storage, ask for timers, check
// settings. Anything a field holds is gone when the instance is evicted.
void yapfast_activate(yf_str payload) {
    (void)payload;
}

// The instance is going away. Answer with what the next activation should be
// given, as a JSON string, or YF_NONE. Storage is what survives, not this.
yf_str yapfast_deactivate(void) {
    return YF_NONE;
}

// One event. `kind` says what it is; a plugin only hears the kinds its manifest
// subscribed to.
void yapfast_event(yf_str event) {
    yf_str kind;
    if (!yf_get(event, YF_STR("kind"), &kind)) return;
    if (yf_string_is(kind, YF_STR("message_create"))) {
        // `payload` is the message itself; read what you need from it and
        // nothing more.
    }
}

// The answer to a call this plugin left pending arrived.
void yapfast_result(uint64_t call, yf_str answer) {
    (void)call;
    (void)answer;
}

// Before a message goes out. Write the outcome into `answer`.
void yapfast_hook(yf_str request, yf_buf *answer) {
    (void)request;
    yf_unchanged(answer);
}

// A command the person typed. Write the outcome into `answer`.
void yapfast_command(yf_str invocation, yf_buf *answer) {
    // The composer checked `name` against the manifest before this ran, so when
    // it is there it is a string. The line is built in a static buffer: a
    // freestanding module has no libc to grow one.
    static char line[512];
    static const char prefix[] = "hello, ";
    size_t at = sizeof(prefix) - 1;
    for (size_t i = 0; i < at; i++) line[i] = prefix[i];

    yf_str name;
    if (yf_path(invocation, YF_STR("options.name"), &name)) {
        ptrdiff_t got = yf_string(name, line + at, sizeof(line) - at);
        if (got < 0) {
            yf_failed(answer, YF_STR("that name is too long to greet"));
            return;
        }
        yf_notice(answer, (yf_str){ line, at + (size_t)got });
        return;
    }
    yf_notice(answer, YF_STR("hello, you"));
}
