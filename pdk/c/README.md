# Plugins in C

A plugin is a WebAssembly module Extism runs, and the ABI between it and Yapfast
is five exports and two imports around JSON. This folder is everything a C plugin
needs to speak it: no C library, no WASI, no SDK, and nothing to install beyond a
clang that can target `wasm32`.

| File | Is |
| --- | --- |
| `yapfast.h` | The PDK: views of bytes, host calls, reading and writing JSON, and the five handlers a plugin writes |
| `yapfast.c` | The five exports, the three buffers, and defaults for handlers a plugin leaves out |
| `json.c` | Scanning JSON as it arrives, and writing small answers back |
| `freestanding.c` | The four memory functions a module with no C library has to bring itself |
| `examples/shout` | A whole plugin: one command, one host call, no state |
| `template` | A copy of the above with nothing in it yet |

```sh
cd examples/shout && ./build.sh && ls -l plugin.wasm
```

## What a plugin writes

Five functions, and only the ones it needs: `yapfast_activate`,
`yapfast_deactivate`, `yapfast_event`, `yapfast_hook` and `yapfast_command`.
`yapfast.c` defines the rest as weak defaults, so a plugin that adds one command
is one function long:

```c
#include "yapfast.h"

void yapfast_command(yf_str invocation, yf_buf *answer) {
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
    yf_notice(answer, (yf_str){ up, (size_t)length });
}
```

`yf_str` is a pointer and a length, never a C string: a payload has no terminator,
because it is the bytes between a frame header and the end of the frame.
`yf_string` unescapes one field into a buffer the plugin owns.

Everything a person reads in the source is a sentence, because a plugin failing
at three in the morning should say why: `yf_failed` is a line they see, and
`yf_notice` is a line in the room they typed in.

## Memory

There is no allocator. The PDK keeps three static buffers — the payload being
read, a scratch answer, and the answer being built — and a plugin that wants more
declares its own array. That is on purpose: an instance's memory is capped by its
manifest, never given back, and a plugin that returns from a call holding less is
a plugin that stays alive longer. Read one event, answer it, and let it go.

A buffer too small isn't a crash: the writer marks itself failed, and the answer
becomes the ABI's default — `unchanged` for a hook, which fails open, and a line
saying the answer was too large for a command.

## Handles are opaque

Extism gives a guest memory through *handles*, and a handle is not a pointer.
What a handle names is Extism's to know: casting one to a `char *` compiles, and
writes bytes that nobody will ever read. The PDK marshals through the kernel's
`store_u64`, `store_u8`, `load_u64` and `load_u8`, eight bytes at a time, which is
why `imports.h` exists and is not something a plugin includes.

## Host calls

`yf_method(YF_STR("storage.get"), YF_STR("{\"key\":\"seen\"}"), out, sizeof(out))`
is one call, and its answer is one of `{"ok": ...}`, `"unsupported"`,
`{"denied": {"reason": ...}}`, `{"pending": {"tag": ...}}` or
`{"error": {"message": ...}}`. A method this build doesn't have answers
`unsupported` rather than failing to link, which is how a plugin asks before it
relies on one. `yf_answer_ok` is the shorthand for the common case.

A call can answer later: `messages.send` and an `http.fetch` that needs a person's
consent both do. The plugin gets `pending` and returns, and the answer arrives
before the events of a later batch, where `yapfast_result` reads it. That is why
an event handler has to handle its own replies.

`yf_log` is a post and never fails a handler: a log line is a hint, and storage
is where a value that matters goes.
