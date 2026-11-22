;; A plugin whose event handler asks Yapfast something, for the path where the
;; answer is not ready yet. `yf_event` calls `yf_call` with an `http.fetch`,
;; which the parent's network gate answers later: the helper forwards the call
;; as a FromHelper::Call frame, the parent answers with a Return, and the answer
;; reaches the guest as a later event. Nothing waits, so the worker that ran
;; this handler is free, and the end-to-end test checks another plugin still
;; runs while this one is unanswered.
;;
;; `yf_call` is Extism's user-namespace host function: it takes the offset of a
;; length-prefixed block holding the HostCall JSON and answers with the offset of
;; one holding the HostReturn JSON. The request lives in this module's own
;; memory and is copied into an allocation, because the kernel's memory is the
;; only place the host reads.
(module
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/user" "yf_call" (func $yf_call (param i64) (result i64)))

  (memory 1)
  ;; The 90 bytes of `{"method":"http.fetch","payload":{"method":"GET",
  ;; "url":"https://api.example.com/version"}}`.
  (data (i32.const 0) "{\"method\":\"http.fetch\",\"payload\":{\"method\":\"GET\",\"url\":\"https://api.example.com/version\"}}")

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event")
    (result i32)
    (local $buffer i64)
    (local $at i64)
    (local.set $buffer (call $alloc (i64.const 90)))
    (block $done
      (loop $copy
        (br_if $done (i64.ge_u (local.get $at) (i64.const 90)))
        (call $store_u8
          (i64.add (local.get $buffer) (local.get $at))
          (i32.load8_u (i32.wrap_i64 (local.get $at))))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $copy)))
    ;; The answer is either immediate or `pending`, and either way this call is
    ;; done: a later event carries the rest.
    (drop (call $yf_call (local.get $buffer)))
    (i32.const 0))
  (func (export "yf_hook") (result i32) (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
