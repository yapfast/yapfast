;; A plugin that answers nonsense where the ABI says a HookOutcome goes: it
;; returns its own question, and a HookRequest is not a HookOutcome. The helper
;; counts that as a FaultKind::BadAnswer and fails open, so the message goes out
;; as it was and the rest of the chain still runs.
;;
;; It answers with the input rather than a fixed literal so the nonsense is the
;; question itself, which no version of the ABI could read as an outcome.
(module
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "input_length" (func $input_length (result i64)))
  (import "extism:host/env" "input_load_u8" (func $input_load_u8 (param i64) (result i32)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))

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

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook")
    (result i32)
    (call $answer_with_input)
    (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
