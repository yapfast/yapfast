// `/hello`: the smallest whole plugin in Go, and the shape to start from.
//
// Yapfast calls five exports on every plugin, and the five functions below name
// them; what the plugin *does* is the handler each one calls. Copy this folder,
// rename the command in `yapfast-plugin.json`, and write `command`.
//
// The ABI is Extism's, so the Extism Go PDK is the PDK: `pdk.Input` and
// `pdk.Output` are the guest's half of it, and the JSON in and out is the
// helper's. Build with TinyGo, never with the standard toolchain — see the
// README. `build.sh` explains the flags and what they cost.
package main

import "github.com/extism/go-pdk"

// The five exports Yapfast calls, named with `//go:wasmexport` because a Go
// module exports only what it marks that way, and the helper refuses a module
// missing one at activation — better then than on the first message.
//
// Each returns Extism's code rather than an answer: 0 is success, and a non-zero
// is a failure with the reason in `pdk.SetErrorString`. What the helper reads is
// the module's *output*, which `pdk.Output*` sets.

//go:wasmexport yf_activate
func activate() int32 {
	// Yapfast gave this instance a new life: read storage, ask for timers,
	// check settings. The input is `{"api":1,"plugin":"…","state":…}`, where
	// `state` is what the last `yf_deactivate` handed on. Anything this file
	// holds is gone when the instance is evicted, so only storage and `state`
	// outlive it — which is why the template keeps nothing.
	return 0
}

//go:wasmexport yf_deactivate
func deactivate() int32 {
	// The instance is going away. Answer what the next activation should be
	// given: `{}` is "nothing", and `{"state":"…"}` is the hand-off, at most
	// 256 KiB of it. Storage is what survives, not this.
	pdk.OutputString(`{}`)
	return 0
}

//go:wasmexport yf_event
func event() int32 {
	// One batch: `{"events":[{"kind":"message_create",…}],"results":[…]}`, where
	// `results` are the answers to host calls that were left pending and
	// `topics` is what other plugins published to topics this one subscribed
	// to. A plugin hears only the kinds its manifest says and the parent's
	// filter already ran, so read the field you need and nothing else —
	// `docs/plugins/abi.md` lists every kind. This template hears none, so it
	// leaves the batch on the floor rather than decoding it.
	//
	// An event's answer is what it did through host calls: nothing reads this
	// export's output, and setting one is a fault, so answer nothing.
	return 0
}

//go:wasmexport yf_hook
func hook() int32 {
	// Before a message goes out: `{"hook":"message_pre_send","subject":…}`. A
	// hook fails open, so the cheapest honest answer is `unchanged`; a plugin
	// that intercepts answers `changed` with the new subject, or `block` with
	// the reason the person sees.
	pdk.OutputString(`"unchanged"`)
	return 0
}

//go:wasmexport yf_command
func command() int32 {
	// The command the manifest declares, as `{"name":"hello","options":{…}}`.
	// The composer checked the options against the manifest before this ran, so
	// an option that is there is the type it says; one that is missing, isn't a
	// string or is empty is "you".
	//
	// A command answers with JSON, and `notice` is the one line the person sees
	// in the room they typed it in; the other answers are `value`, the plugin's
	// own JSON, and `failed` with a message. The name is spliced into the answer
	// without being unescaped first: an option arrives as a JSON string and
	// leaves as one, so its escapes are already what the frame needs.
	name := []byte("you")
	if given, ok := option(pdk.Input(), "name"); ok && len(given) > 0 {
		name = given
	}
	pdk.OutputString(`{"notice":{"text":"hello, ` + string(name) + `"}}`)
	return 0
}

// option is the body of the JSON string that `invocation`'s `options` holds
// under `key` — `Ada`, from `{"name":"hello","options":{"name":"Ada"}}` — or
// false when there is no such option or it isn't a string.
//
// This is the whole of the JSON a plugin needs, written out because
// `encoding/json` is TinyGo's reflect: decoding this invocation with it makes
// the module 409.0 KiB instead of 10.2, which is the difference the README's
// numbers are about. A plugin that wants real JSON in a payload should weigh
// that the way the README does.
func option(invocation []byte, key string) ([]byte, bool) {
	options, ok := member(invocation, "options")
	if !ok {
		return nil, false
	}
	value, ok := member(options, key)
	if !ok || len(value) < 2 || value[0] != '"' {
		return nil, false
	}
	return value[1 : len(value)-1], true
}

// member finds one key of a JSON object and answers its value, whole and still
// as JSON. It walks the object and skips every value it isn't asked about, so
// a key that isn't there costs a scan and no allocation.
func member(object []byte, key string) ([]byte, bool) {
	at := skipSpace(object, 0)
	if at >= len(object) || object[at] != '{' {
		return nil, false
	}
	at++
	for {
		at = skipSpace(object, at)
		if at >= len(object) || object[at] != '"' {
			return nil, false
		}
		end, ok := skipString(object, at)
		if !ok {
			return nil, false
		}
		name := object[at+1 : end-1]
		at = skipSpace(object, end)
		if at >= len(object) || object[at] != ':' {
			return nil, false
		}
		start := skipSpace(object, at+1)
		end, ok = skipValue(object, start)
		if !ok {
			return nil, false
		}
		if equal(name, key) {
			return object[start:end], true
		}
		at = skipSpace(object, end)
		if at < len(object) && object[at] == ',' {
			at++
			continue
		}
		return nil, false
	}
}

// skipSpace answers the offset of the first byte at or after `at` that isn't
// JSON's whitespace.
func skipSpace(b []byte, at int) int {
	for at < len(b) {
		switch b[at] {
		case ' ', '\t', '\n', '\r':
			at++
		default:
			return at
		}
	}
	return at
}

// skipString answers the offset just past the string that starts at `at`,
// stepping over an escaped byte inside it so `\"` doesn't end it.
func skipString(b []byte, at int) (int, bool) {
	for at++; at < len(b); at++ {
		switch b[at] {
		case '\\':
			at++
		case '"':
			return at + 1, true
		}
	}
	return 0, false
}

// skipValue answers the offset just past the JSON value that starts at `at`.
// A number, `true`, `false` and `null` all end where a value ends and need no
// telling apart, so they are one case.
func skipValue(b []byte, at int) (int, bool) {
	if at >= len(b) {
		return 0, false
	}
	switch b[at] {
	case '"':
		return skipString(b, at)
	case '{', '[':
		depth := 0
		for ; at < len(b); at++ {
			switch b[at] {
			case '"':
				end, ok := skipString(b, at)
				if !ok {
					return 0, false
				}
				at = end - 1
			case '{', '[':
				depth++
			case '}', ']':
				depth--
				if depth == 0 {
					return at + 1, true
				}
			}
		}
		return 0, false
	default:
		for at < len(b) {
			switch b[at] {
			case ',', '}', ']', ' ', '\t', '\n', '\r':
				return at, true
			}
			at++
		}
		return at, true
	}
}

// equal reports whether b is exactly key, without a conversion that allocates.
func equal(b []byte, key string) bool {
	if len(b) != len(key) {
		return false
	}
	for i := 0; i < len(key); i++ {
		if b[i] != key[i] {
			return false
		}
	}
	return true
}

// `main` is never called: a `c-shared` build is a reactor, and Yapfast calls the
// five exports above. TinyGo still wants one to link a Go program.
func main() {}
