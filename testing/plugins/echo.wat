;; The well-behaved plugin the other scenarios start from. `yf_activate`
;; succeeds and does nothing, and `yf_event` answers with the batch it was
;; given, so a test can see the bytes it sent come back through the pipe, the
;; scheduler, the runtime and the guest's Extism memory.
;;
;; Extism calls an export with no arguments and reads its answer from the
;; kernel's memory, so every export here takes nothing and returns a status:
;; 0 means the call succeeded. `yf_hook` answers `"unchanged"` because a hook
;; chain may include this plugin; the exports the scenarios never reach return
;; 0 with no answer, which the ABI reads as nothing.
(module
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "input_length" (func $input_length (result i64)))
  (import "extism:host/env" "input_load_u8" (func $input_load_u8 (param i64) (result i32)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/env" "store_u64" (func $store_u64 (param i64 i64)))
  (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))

  ;; Copies the input buffer into a fresh allocation and makes that the answer.
  ;; `alloc` stamps the block's length, which is what the host reads, so the
  ;; copy needs no length arithmetic of its own; a zero length allocates
  ;; nothing and leaves the answer empty.
  (func $answer_with_input
    (local $length i64)
    (local $buffer i64)
    (local $at i64)
    (local.set $length (call $input_length))
    (local.set $buffer (call $alloc (local.get $length)))
    (block $done
      (loop $copy
        (br_if $done (i64.ge_u (local.get $at) (local.get $length)))
        (call $store_u8
          (i64.add (local.get $buffer) (local.get $at))
          (call $input_load_u8 (local.get $at)))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $copy)))
    (call $output_set (local.get $buffer) (local.get $length)))

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
  (func (export "yf_event")
    (result i32)
    (call $answer_with_input)
    (i32.const 0))
  (func (export "yf_hook")
    (result i32)
    (call $answer_unchanged)
    (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
