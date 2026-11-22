;; A hook that reads one of its settings and answers with it: the path a PDK's
;; `settings::get` takes inside `yf_hook`. `yf_call` asks for the setting
;; `signature`, and the guest waits for the parent's answer rather than being
;; told `pending`, so the answer it reads back is `{"ok":<value>}`. The value,
;; a JSON string, is copied out of that answer as it stands and becomes the
;; draft's body:
;;   {"changed":{"draft":{"room":"room-1","body":<value>,"attachments":[]}}}
;; An answer of `pending` would leave the body as `ding":{"tag":"1"}`, which
;; isn't JSON, so a guest that wasn't given the value fails the hook rather
;; than passing the test.
(module
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "length" (func $length (param i64) (result i64)))
  (import "extism:host/env" "load_u8" (func $load_u8 (param i64) (result i32)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))
  (import "extism:host/user" "yf_call" (func $yf_call (param i64) (result i64)))

  (memory 1)
  ;; The 55 bytes of the call.
  (data (i32.const 0) "{\"method\":\"settings.get\",\"payload\":{\"key\":\"signature\"}}")
  ;; The 44 bytes before the value.
  (data (i32.const 64) "{\"changed\":{\"draft\":{\"room\":\"room-1\",\"body\":")
  ;; The 20 bytes after it.
  (data (i32.const 128) ",\"attachments\":[]}}}")

  ;; Copies `count` bytes of this module's memory from `from` to the kernel's
  ;; at `to`.
  (func $put (param $to i64) (param $from i32) (param $count i32)
    (local $at i32)
    (block $done
      (loop $copy
        (br_if $done (i32.ge_u (local.get $at) (local.get $count)))
        (call $store_u8
          (i64.add (local.get $to) (i64.extend_i32_u (local.get $at)))
          (i32.load8_u (i32.add (local.get $from) (local.get $at))))
        (local.set $at (i32.add (local.get $at) (i32.const 1)))
        (br $copy))))

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook")
    (result i32)
    (local $request i64)
    (local $answer i64)
    (local $value i64)
    (local $out i64)
    (local $at i64)
    (local.set $request (call $alloc (i64.const 55)))
    (call $put (local.get $request) (i32.const 0) (i32.const 55))
    (local.set $answer (call $yf_call (local.get $request)))
    ;; `{"ok":` is six bytes and the closing brace one.
    (local.set $value (i64.sub (call $length (local.get $answer)) (i64.const 7)))
    (local.set $out (call $alloc (i64.add (local.get $value) (i64.const 64))))
    (call $put (local.get $out) (i32.const 64) (i32.const 44))
    (block $done
      (loop $copy
        (br_if $done (i64.ge_u (local.get $at) (local.get $value)))
        (call $store_u8
          (i64.add (local.get $out) (i64.add (i64.const 44) (local.get $at)))
          (call $load_u8 (i64.add (local.get $answer) (i64.add (i64.const 6) (local.get $at)))))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $copy)))
    (call $put
      (i64.add (local.get $out) (i64.add (i64.const 44) (local.get $value)))
      (i32.const 128)
      (i32.const 20))
    (call $output_set (local.get $out) (i64.add (local.get $value) (i64.const 64)))
    (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
