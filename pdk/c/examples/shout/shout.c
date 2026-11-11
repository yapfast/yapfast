//! `/shout`: send the text the person gave in capitals.
//!
//! The smallest useful plugin: one command, one host call, no state. It shows the
//! three things every plugin does — read what it was given, do something through
//! the host, and say what the person should see.

#include "../../yapfast.h"

#ifndef YF_ANSWER_MAX
#define YF_ANSWER_MAX 4096
#endif

static char answer_payload[YF_ANSWER_MAX];

void yapfast_command(yf_str invocation, yf_buf *answer) {
    // The composer checked `text` against the manifest before the plugin was
    // called, so it is there and it is a string; the failure below is what a
    // person sees if it somehow isn't.
    yf_str text;
    if (!yf_path(invocation, YF_STR("options.text"), &text)) {
        yf_failed(answer, YF_STR("say what to shout"));
        return;
    }
    char up[1024];
    ptrdiff_t length = yf_string(text, up, sizeof(up));
    if (length < 0) {
        yf_failed(answer, YF_STR("that text is too long to shout"));
        return;
    }
    for (ptrdiff_t i = 0; i < length; i++) {
        if (up[i] >= 'a' && up[i] <= 'z') up[i] = (char)(up[i] - 'a' + 'A');
    }

    // `messages.send` needs the capability of the same name, which this plugin's
    // manifest declares. The helper sends into the room the command was typed
    // in: nothing else names a room.
    yf_buf body;
    yf_buf_init(&body, answer_payload, sizeof(answer_payload));
    yf_raw(&body, YF_STR("{\"body\":"));
    yf_quoted(&body, (yf_str){ up, (size_t)length });
    yf_raw(&body, YF_STR(",\"reply\":true}"));
    if (body.failed) {
        yf_failed(answer, YF_STR("that text is too long to shout"));
        return;
    }

    // A send is a call the host answers later, on the instance's next event, so
    // `pending` is success here. Any other answer is a refusal or a failure, and
    // the person hears about it instead of a message going nowhere.
    char reply[1024];
    ptrdiff_t got = yf_method(YF_STR("messages.send"),
                              (yf_str){ answer_payload, body.len },
                              reply,
                              sizeof(reply));
    if (got < 0) {
        yf_failed(answer, YF_STR("Yapfast didn't answer the send"));
        return;
    }
    yf_str handed;
    if (yf_answer_ok((yf_str){ reply, (size_t)got }, &handed) ||
        yf_get((yf_str){ reply, (size_t)got }, YF_STR("pending"), &handed)) {
        yf_none(answer);
        return;
    }
    yf_failed(answer, YF_STR("Yapfast wouldn't send that"));
}
