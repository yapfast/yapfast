;; The well-behaved plugin the other scenarios start from. `yf_activate`
;; succeeds and does nothing, and `yf_event` takes its batch and answers
;; nothing, which is the only answer the ABI gives an event: an answer of its
;; own would be a `bad_answer` fault and cost the instance.
;;
;; Extism calls an export with no arguments and reads its answer from the
;; kernel's memory, so every export here takes nothing and returns a status:
;; 0 means the call succeeded. `yf_hook` answers `"unchanged"` because a hook
;; chain may include this plugin; the exports the scenarios never reach return
;; 0 with no answer, which the ABI reads as nothing.
(module
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/env" "store_u64" (func $store_u64 (param i64 i64)))
  (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))

  ;; `"unchanged"`, the HookOutcome that leaves the draft as it was. The eight
  ;; bytes from offset 0 of the literal are `"unchang`; the rest follow.
  (func $answer_unchanged
    (local $buffer i64)
    (local.set $buffer (call $alloc (i64.const 11)))
    (call $store_u64 (local.get $buffer) (i64.const 0x676e6168636e7522))
    (call $store_u8 (i64.add (local.get $buffer) (i64.const 8)) (i32.const 101))
    (call $store_u8 (i64.add (local.get $buffer) (i64.const 9)) (i32.const 100))
    (call $store_u8 (i64.add (local.get $buffer) (i64.const 10)) (i32.const 34))
    (call $output_set (local.get $buffer) (i64.const 11)))

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook")
    (result i32)
    (call $answer_unchanged)
    (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
